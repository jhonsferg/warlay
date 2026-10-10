use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use base64::Engine;
use redis::AsyncCommands;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn hash_password(password: &str) -> ApiResult<String> {
    let mut salt_bytes = [0_u8; 16];
    getrandom::fill(&mut salt_bytes).map_err(ApiError::internal)?;
    let salt = SaltString::encode_b64(&salt_bytes).map_err(ApiError::internal)?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(ApiError::internal)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
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
    let hits: u64 = redis.incr(&key, 1).await?;
    if hits == 1 {
        let _: () = redis.expire(&key, window_secs).await?;
    }
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
        let hash = hash_password("correct horse").unwrap();
        assert!(verify_password("correct horse", &hash));
        assert!(!verify_password("wrong horse", &hash));
        assert!(!verify_password("correct horse", "not-a-hash"));
    }

    #[test]
    fn tokens_are_random_and_hashed() {
        let (a, b) = (random_token(), random_token());
        assert_ne!(a, b);
        assert_eq!(token_hash(&a).len(), 32);
        assert_ne!(token_hash(&a), token_hash(&b));
    }
}
