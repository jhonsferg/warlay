use argon2::password_hash::{Output, ParamsString, PasswordHash, SaltString};
use argon2::{Algorithm, Argon2, Block, Params, Version};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use base64::Engine;
use redis::AsyncCommands;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

type Argon2Job = Box<dyn FnOnce() + Send>;

/// Password hashing runs on dedicated threads (half the cores, so request
/// handling keeps the rest during a login burst): each hash takes tens of
/// milliseconds of CPU, and must neither stall the async workers nor spread
/// over many threads, each keeping its own 19 MiB of working memory.
static ARGON2_THREADS: LazyLock<usize> =
    LazyLock::new(|| std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(2)));

/// Hashes waiting for a thread. Past `ARGON2_THREADS * QUEUE_PER_THREAD`
/// (about two seconds of work) new ones are refused at once: under a burst
/// the alternative is clients timing out while their hash still runs, then
/// retrying -- more work for requests nobody is waiting for any more.
static ARGON2_QUEUED: AtomicUsize = AtomicUsize::new(0);
const QUEUE_PER_THREAD: usize = 48;

static ARGON2_POOL: LazyLock<Mutex<std::sync::mpsc::Sender<Argon2Job>>> = LazyLock::new(|| {
    let (tx, rx) = std::sync::mpsc::channel::<Argon2Job>();
    let rx = Arc::new(Mutex::new(rx));
    for i in 0..*ARGON2_THREADS {
        let rx = rx.clone();
        std::thread::Builder::new()
            .name(format!("argon2-{i}"))
            .spawn(move || loop {
                let job = rx.lock().unwrap().recv();
                match job {
                    Ok(job) => job(),
                    Err(_) => return,
                }
            })
            .expect("spawning an argon2 thread");
    }
    Mutex::new(tx)
});

/// Checked against when a login names no account, so unknown and known
/// accounts take the same time to refuse. Only touched on the argon2 pool.
static DUMMY_PASSWORD_HASH: LazyLock<String> =
    LazyLock::new(|| hash_password_blocking("warlay-dummy-password").expect("hashing a constant"));

fn busy() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "busy",
        "el servidor está muy ocupado, intenta de nuevo en unos segundos",
    )
}

/// Refuses up front (503) when the argon2 pool is too far behind. Called
/// before rate limiting, so retrying a refused login doesn't use up the
/// account's attempts.
pub fn ensure_hashing_capacity() -> ApiResult<()> {
    if ARGON2_QUEUED.load(Ordering::Relaxed) >= *ARGON2_THREADS * QUEUE_PER_THREAD {
        return Err(busy());
    }
    Ok(())
}

/// Runs a password hash on the argon2 pool, off the async workers; refuses
/// it (503, try again shortly) when the pool is too far behind.
async fn run_argon2<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> ApiResult<T> {
    if ARGON2_QUEUED.fetch_add(1, Ordering::Relaxed) >= *ARGON2_THREADS * QUEUE_PER_THREAD {
        ARGON2_QUEUED.fetch_sub(1, Ordering::Relaxed);
        return Err(busy());
    }
    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    let job: Argon2Job = Box::new(move || {
        ARGON2_QUEUED.fetch_sub(1, Ordering::Relaxed);
        // Its caller timed out or went away: skip the work.
        if !done_tx.is_closed() {
            let _ = done_tx.send(work());
        }
    });
    ARGON2_POOL
        .lock()
        .unwrap()
        .send(job)
        .map_err(|_| ApiError::internal("the argon2 pool is gone"))?;
    done_rx.await.map_err(ApiError::internal)
}

pub async fn hash_password(password: &str) -> ApiResult<String> {
    let password = password.to_owned();
    run_argon2(move || hash_password_blocking(&password)).await?
}

/// Checks `password` against `hash`; with no hash (no such account) it
/// still spends the time of a check, against a dummy hash, and fails. Errs
/// only when the check couldn't run (the server is too busy).
pub async fn verify_password(password: &str, hash: Option<&str>) -> ApiResult<bool> {
    let (password, hash) = (password.to_owned(), hash.map(str::to_owned));
    run_argon2(move || match hash {
        Some(hash) => verify_password_blocking(&password, &hash),
        None => {
            verify_password_blocking(&password, &DUMMY_PASSWORD_HASH);
            false
        }
    })
    .await
}

thread_local! {
    /// Each argon2 thread's working memory (19 MiB with the default
    /// parameters), allocated once and reused by every hash. Allocating it
    /// per hash, glibc stops returning it to the system after the first
    /// frees and fragments it across arenas: a 2000-login burst left the
    /// main server holding ~1.7 GiB.
    static ARGON2_MEMORY: RefCell<Vec<Block>> = const { RefCell::new(Vec::new()) };
}

/// Argon2 of `password` into `out`, on this thread's reusable memory.
fn argon2_into(argon: &Argon2, params: &Params, password: &[u8], salt: &[u8], out: &mut [u8]) -> argon2::Result<()> {
    ARGON2_MEMORY.with_borrow_mut(|memory| {
        let blocks = params.block_count();
        if memory.len() < blocks {
            memory.resize(blocks, Block::default());
        }
        argon.hash_password_into_with_memory(password, salt, out, &mut memory[..blocks])
    })
}

/// A PHC string (`$argon2id$v=19$m=...`), the same `Argon2::default()`'s
/// `PasswordHasher` produces.
fn hash_password_blocking(password: &str) -> ApiResult<String> {
    let mut salt_bytes = [0_u8; 16];
    getrandom::fill(&mut salt_bytes).map_err(ApiError::internal)?;
    let salt = SaltString::encode_b64(&salt_bytes).map_err(ApiError::internal)?;
    let (algorithm, version, params) = (Algorithm::default(), Version::default(), Params::default());
    let mut out = [0_u8; Params::DEFAULT_OUTPUT_LEN];
    argon2_into(
        &Argon2::new(algorithm, version, params.clone()),
        &params,
        password.as_bytes(),
        &salt_bytes,
        &mut out,
    )
    .map_err(ApiError::internal)?;
    let hash = PasswordHash {
        algorithm: algorithm.ident(),
        version: Some(version.into()),
        params: ParamsString::try_from(&params).map_err(ApiError::internal)?,
        salt: Some(salt.as_salt()),
        hash: Some(Output::new(&out).map_err(ApiError::internal)?),
    };
    Ok(hash.to_string())
}

/// Checks `password` against a PHC string, honoring the parameters stored in
/// it (so hashes made with other parameters keep verifying).
fn verify_password_blocking(password: &str, hash: &str) -> bool {
    let check = || -> Option<bool> {
        let parsed = PasswordHash::new(hash).ok()?;
        let algorithm = Algorithm::try_from(parsed.algorithm).ok()?;
        let version = parsed.version.map_or(Ok(Version::default()), Version::try_from).ok()?;
        let params = Params::try_from(&parsed).ok()?;
        let expected = parsed.hash?;
        let mut salt_buf = [0_u8; 64];
        let salt = parsed.salt?.decode_b64(&mut salt_buf).ok()?;
        let mut out = vec![0_u8; expected.len()];
        argon2_into(
            &Argon2::new(algorithm, version, params.clone()),
            &params,
            password.as_bytes(),
            salt,
            &mut out,
        )
        .ok()?;
        // `Output`'s equality is constant-time.
        Some(Output::new(&out).ok()? == expected)
    };
    check().unwrap_or(false)
}

/// 32 random bytes, URL-safe base64 -- for access and refresh tokens.
pub fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).expect("the OS random number generator is unavailable");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Tokens are only ever stored as their SHA-256.
pub fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn access_key(token: &str) -> String {
    let hash: String = token_hash(token).iter().map(|b| format!("{b:02x}")).collect();
    format!("wl:access:{hash}")
}

/// Issues a short-lived access token for `session_id`, kept in Redis only.
pub async fn issue_access_token(state: &AppState, user_id: Uuid, session_id: Uuid) -> ApiResult<String> {
    let token = random_token();
    let mut redis = state.redis.clone();
    let _: () = redis
        .set_ex(
            access_key(&token),
            format!("{user_id}:{session_id}"),
            state.settings.access_token_ttl_secs,
        )
        .await?;
    Ok(token)
}

pub async fn revoke_access_token(state: &AppState, token: &str) -> ApiResult<()> {
    let mut redis = state.redis.clone();
    let _: () = redis.del(access_key(token)).await?;
    Ok(())
}

/// Resolves an access token to `(user_id, session_id)`.
pub async fn resolve_access_token(state: &AppState, token: &str) -> ApiResult<Option<(Uuid, Uuid)>> {
    let mut redis = state.redis.clone();
    let value: Option<String> = redis.get(access_key(token)).await?;
    Ok(value.and_then(|v| {
        let (user, session) = v.split_once(':')?;
        Some((Uuid::parse_str(user).ok()?, Uuid::parse_str(session).ok()?))
    }))
}

/// The authenticated caller, extracted from `Authorization: Bearer <token>`.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub token: String,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::to_owned)
            .ok_or_else(ApiError::unauthorized)?;
        let (user_id, session_id) = resolve_access_token(state, &token)
            .await?
            .ok_or_else(ApiError::unauthorized)?;
        Ok(Self {
            user_id,
            session_id,
            token,
        })
    }
}

/// Fixed-window rate limit in Redis: at most `max` hits per `window_secs`.
pub async fn rate_limit(state: &AppState, key: &str, max: u64, window_secs: i64) -> ApiResult<()> {
    let mut redis = state.redis.clone();
    let key = format!("wl:rl:{key}");
    // One round trip, and the window always gets its expiry (`NX`: only if
    // it has none yet), even if an earlier call failed between the two.
    let (hits,): (u64,) = redis::pipe()
        .incr(&key, 1)
        .cmd("EXPIRE")
        .arg(&key)
        .arg(window_secs)
        .arg("NX")
        .ignore()
        .query_async(&mut redis)
        .await?;
    if hits > max {
        return Err(ApiError::new(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "demasiados intentos, espera unos minutos",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_hash_and_verify() {
        let hash = hash_password_blocking("correct horse").unwrap();
        assert!(verify_password_blocking("correct horse", &hash));
        assert!(!verify_password_blocking("wrong horse", &hash));
        assert!(!verify_password_blocking("correct horse", "not-a-hash"));
    }

    #[test]
    fn hashes_match_the_standard_argon2_implementation() {
        use argon2::password_hash::{PasswordHasher, PasswordVerifier};
        // Ours verifies with the crate's verifier...
        let ours = hash_password_blocking("correct horse").unwrap();
        let parsed = PasswordHash::new(&ours).unwrap();
        assert!(Argon2::default().verify_password(b"correct horse", &parsed).is_ok());
        assert!(ours.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        // ...and hashes from the crate (existing accounts) verify with ours,
        // including ones made with other parameters.
        let salt = SaltString::encode_b64(b"0123456789abcdef").unwrap();
        let standard = Argon2::default()
            .hash_password(b"correct horse", &salt)
            .unwrap()
            .to_string();
        assert!(verify_password_blocking("correct horse", &standard));
        assert!(!verify_password_blocking("wrong horse", &standard));
        let cheap = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(1024, 1, 1, None).unwrap(),
        );
        let other = cheap.hash_password(b"correct horse", &salt).unwrap().to_string();
        assert!(verify_password_blocking("correct horse", &other));
    }

    #[tokio::test]
    async fn the_argon2_pool_serves_concurrent_requests() {
        let hashes = futures_util::future::join_all(
            (0..3).map(|i| async move { (i, hash_password(&format!("password {i}")).await.unwrap()) }),
        )
        .await;
        for (i, hash) in &hashes {
            assert!(verify_password(&format!("password {i}"), Some(hash)).await.unwrap());
            assert!(!verify_password("something else", Some(hash)).await.unwrap());
        }
        // No account: still refused (after spending a check's time).
        assert!(!verify_password("password 0", None).await.unwrap());
    }

    #[test]
    fn tokens_are_random_and_hashed() {
        let (a, b) = (random_token(), random_token());
        assert_ne!(a, b);
        assert_eq!(token_hash(&a).len(), 32);
        assert_ne!(token_hash(&a), token_hash(&b));
    }
}
