//! Sign-in with Google, for the desktop app.
//!
//! The app opens the system browser on `/auth/google/start` and waits on a
//! loopback port (RFC 8252). Google sends the browser back to
//! `/auth/google/callback`; the server exchanges Google's code for the player's
//! identity and sends the browser on to `http://127.0.0.1:{port}/callback` with
//! a one-time Warlay code. The app trades that code (plus its PKCE verifier)
//! for a session at `POST /api/v1/auth/google/exchange`, so the code is useless
//! to anything else that could see the loopback URL.
//!
//! An account can have a password, Google, or both. A Google identity is
//! matched by Google's stable subject id; an existing account is matched by
//! e-mail only after its password is confirmed, because Warlay never verified
//! those e-mails.

use axum::extract::{Form, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::{
    ensure_hashing_capacity, hash_password, random_token, rate_limit, token_hash, verify_password, ClientIp,
};
use crate::error::{ApiError, ApiResult};
use crate::models::{load_user, pick_color};
use crate::routes::auth::{start_session, AuthResponse};
use crate::state::AppState;
use crate::web::i18n::{negotiate, Lang};
use crate::web::{base, html, html_sign_in_form, pages};

const PROVIDER: &str = "google";
const COOKIE: &str = "wl_oauth";
/// How long a sign-in may take, from the button to the end.
const FLOW_TTL_SECS: u64 = 600;
/// How long the app has to trade the one-time code.
const CODE_TTL_SECS: u64 = 120;

fn enabled(state: &AppState) -> bool {
    !state.settings.google_client_id.is_empty() && !state.settings.google_client_secret.is_empty()
}

/// What the app asked for, kept in Redis while the player is at Google.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Flow {
    port: u16,
    state: String,
    challenge: String,
    lang: String,
}

impl Flow {
    fn lang(&self) -> Lang {
        negotiate(Some(&self.lang), None)
    }

    fn loopback(&self, query: &[(&str, &str)]) -> String {
        let mut url = url::Url::parse(&format!("http://127.0.0.1:{}/callback", self.port)).expect("valid loopback url");
        url.query_pairs_mut()
            .append_pair("state", &self.state)
            .extend_pairs(query.iter().copied());
        url.into()
    }
}

/// A Google identity waiting for the player to prove they own the account
/// with the same e-mail.
#[derive(Debug, Serialize, Deserialize)]
struct PendingLink {
    flow: Flow,
    nonce: String,
    subject: String,
    email: String,
    user_id: Uuid,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Providers {
    google: bool,
    /// The website the app opens in the browser to sign in.
    web_url: String,
}

/// Which sign-in methods besides the password this server offers, so the app
/// shows only buttons that work.
pub async fn providers(State(state): State<AppState>) -> Json<Providers> {
    Json(Providers {
        google: enabled(&state),
        web_url: base(&state).to_string(),
    })
}

// --- Redis helpers -------------------------------------------------------

async fn put<T: Serialize>(state: &AppState, key: &str, value: &T, ttl: u64) -> ApiResult<()> {
    let mut redis = state.redis.clone();
    let json = serde_json::to_string(value).map_err(ApiError::internal)?;
    let _: () = redis::cmd("SET")
        .arg(key)
        .arg(json)
        .arg("EX")
        .arg(ttl)
        .query_async(&mut redis)
        .await?;
    Ok(())
}

async fn take<T: for<'de> Deserialize<'de>>(state: &AppState, key: &str) -> ApiResult<Option<T>> {
    let mut redis = state.redis.clone();
    let json: Option<String> = redis::cmd("GETDEL").arg(key).query_async(&mut redis).await?;
    Ok(json.and_then(|json| serde_json::from_str(&json).ok()))
}

async fn peek<T: for<'de> Deserialize<'de>>(state: &AppState, key: &str) -> ApiResult<Option<T>> {
    let mut redis = state.redis.clone();
    let json: Option<String> = redis::cmd("GET").arg(key).query_async(&mut redis).await?;
    Ok(json.and_then(|json| serde_json::from_str(&json).ok()))
}

async fn drop_key(state: &AppState, key: &str) -> ApiResult<()> {
    let mut redis = state.redis.clone();
    let _: () = redis::cmd("DEL").arg(key).query_async(&mut redis).await?;
    Ok(())
}

fn flow_key(nonce: &str) -> String {
    format!("wl:oauth:flow:{nonce}")
}

fn link_key(id: &str) -> String {
    format!("wl:oauth:link:{id}")
}

fn code_key(code: &str) -> String {
    let digest = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_hash(code));
    format!("wl:oauth:code:{digest}")
}

// --- Cookie --------------------------------------------------------------

/// The cookie ties the callback to the browser that pressed the button, so a
/// link someone else started cannot be completed in this browser.
fn set_cookie(state: &AppState, value: &str, max_age: u64) -> HeaderValue {
    let secure = if state.settings.web_base_url.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    HeaderValue::from_str(&format!(
        "{COOKIE}={value}; Max-Age={max_age}; Path=/auth/google; HttpOnly; SameSite=Lax{secure}"
    ))
    .expect("cookie is ascii")
}

fn cookie_of(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().strip_prefix(&format!("{COOKIE}=")).map(str::to_string))
        .next()
}

fn with_cookie(mut response: Response, cookie: HeaderValue) -> Response {
    response.headers_mut().append(header::SET_COOKIE, cookie);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn page_lang(headers: &HeaderMap) -> Lang {
    negotiate(None, headers.get(header::ACCEPT_LANGUAGE).and_then(|v| v.to_str().ok()))
}

fn error_page(state: &AppState, lang: Lang, status: StatusCode, detail: &str) -> Response {
    let mut response = html(pages::auth_error(base(state), lang, detail), lang);
    *response.status_mut() = status;
    with_cookie(response, set_cookie(state, "", 0))
}

fn text(lang: Lang, es: &'static str, en: &'static str) -> &'static str {
    match lang {
        Lang::Es => es,
        Lang::En => en,
    }
}

// --- Step 1: the app sends the browser here -------------------------------

#[derive(Debug, Deserialize)]
pub struct StartQuery {
    port: Option<u16>,
    state: Option<String>,
    challenge: Option<String>,
    lang: Option<String>,
}

fn is_url_safe(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub async fn start(State(state): State<AppState>, Query(q): Query<StartQuery>, headers: HeaderMap) -> Response {
    let lang = negotiate(
        q.lang.as_deref(),
        headers.get(header::ACCEPT_LANGUAGE).and_then(|v| v.to_str().ok()),
    );
    if !enabled(&state) {
        return error_page(
            &state,
            lang,
            StatusCode::NOT_FOUND,
            text(
                lang,
                "Este servidor no tiene activado el inicio de sesión con Google.",
                "This server does not have Google sign-in turned on.",
            ),
        );
    }
    // Only the loopback of the player's own machine is ever a destination, so
    // the port is the only thing the caller picks there.
    let (Some(port), Some(app_state), Some(challenge)) = (q.port, q.state, q.challenge) else {
        return error_page(
            &state,
            lang,
            StatusCode::BAD_REQUEST,
            text(
                lang,
                "Abre esta página desde el botón \"Continuar con Google\" de Warlay.",
                "Open this page from the \"Continue with Google\" button in Warlay.",
            ),
        );
    };
    if port < 1024 || !is_url_safe(&app_state, 16, 128) || !is_url_safe(&challenge, 43, 128) {
        return error_page(
            &state,
            lang,
            StatusCode::BAD_REQUEST,
            text(lang, "La solicitud no es válida.", "The request is not valid."),
        );
    }
    let nonce = random_token();
    let flow = Flow {
        port,
        state: app_state,
        challenge,
        lang: lang.code().to_string(),
    };
    if put(&state, &flow_key(&nonce), &flow, FLOW_TTL_SECS).await.is_err() {
        return error_page(
            &state,
            lang,
            StatusCode::SERVICE_UNAVAILABLE,
            text(
                lang,
                "El servicio no está disponible ahora.",
                "The service is not available right now.",
            ),
        );
    }
    let mut url = url::Url::parse(&state.settings.google_auth_url).expect("google_auth_url is a url");
    url.query_pairs_mut()
        .append_pair("client_id", &state.settings.google_client_id)
        .append_pair("redirect_uri", &redirect_uri(&state))
        .append_pair("response_type", "code")
        .append_pair("scope", "openid email profile")
        .append_pair("state", &nonce)
        .append_pair("prompt", "select_account");
    with_cookie(
        Redirect::to(url.as_str()).into_response(),
        set_cookie(&state, &nonce, FLOW_TTL_SECS),
    )
}

fn redirect_uri(state: &AppState) -> String {
    format!("{}/auth/google/callback", base(state))
}

// --- Step 2: Google sends the browser back --------------------------------

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Debug)]
struct GoogleIdentity {
    subject: String,
    email: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: String,
}

#[derive(Debug, Deserialize)]
struct IdClaims {
    iss: String,
    aud: String,
    sub: String,
    exp: i64,
    email: Option<String>,
    /// Google sends a boolean; some clients a string.
    email_verified: Option<serde_json::Value>,
    name: Option<String>,
}

/// Trades Google's code for the player's identity. The ID token comes straight
/// from Google's token endpoint over TLS, which is the case where the OpenID
/// Connect spec lets the client skip checking its signature; the claims that
/// bind it to this client and this moment are still checked.
async fn google_identity(state: &AppState, code: &str) -> Result<GoogleIdentity, &'static str> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|_| "client")?;
    let response = client
        .post(&state.settings.google_token_url)
        .form(&[
            ("code", code),
            ("client_id", state.settings.google_client_id.as_str()),
            ("client_secret", state.settings.google_client_secret.as_str()),
            ("redirect_uri", redirect_uri(state).as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|error| {
            tracing::warn!(%error, "google token request failed");
            "provider_unreachable"
        })?;
    if !response.status().is_success() {
        tracing::warn!(status = %response.status(), "google refused the authorization code");
        return Err("provider_refused");
    }
    let token: TokenResponse = response.json().await.map_err(|_| "provider_invalid")?;
    parse_id_token(
        &token.id_token,
        &state.settings.google_client_id,
        Utc::now().timestamp(),
    )
}

fn parse_id_token(id_token: &str, client_id: &str, now: i64) -> Result<GoogleIdentity, &'static str> {
    let payload = id_token.split('.').nth(1).ok_or("provider_invalid")?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| "provider_invalid")?;
    let claims: IdClaims = serde_json::from_slice(&bytes).map_err(|_| "provider_invalid")?;
    if !matches!(
        claims.iss.as_str(),
        "https://accounts.google.com" | "accounts.google.com"
    ) || claims.aud != client_id
        || claims.exp <= now
        || claims.sub.is_empty()
    {
        return Err("provider_invalid");
    }
    let verified = match &claims.email_verified {
        Some(serde_json::Value::Bool(v)) => *v,
        Some(serde_json::Value::String(v)) => v == "true",
        _ => false,
    };
    let email = claims.email.map(|e| e.trim().to_lowercase()).unwrap_or_default();
    if email.is_empty() || !verified {
        return Err("email_not_verified");
    }
    Ok(GoogleIdentity {
        subject: claims.sub,
        email,
        name: claims.name.unwrap_or_default(),
    })
}

/// Back to the app: the browser goes to its loopback listener with the answer.
fn to_app(state: &AppState, flow: &Flow, query: &[(&str, &str)]) -> Response {
    with_cookie(
        Redirect::to(&flow.loopback(query)).into_response(),
        set_cookie(state, "", 0),
    )
}

/// Records the one-time code the app will trade, and sends the browser to it.
async fn finish(state: &AppState, flow: &Flow, user_id: Uuid) -> Response {
    let code = random_token();
    let stored = put(
        state,
        &code_key(&code),
        &CodeRecord {
            user_id,
            challenge: flow.challenge.clone(),
        },
        CODE_TTL_SECS,
    )
    .await;
    match stored {
        Ok(()) => to_app(state, flow, &[("code", &code)]),
        Err(_) => to_app(state, flow, &[("error", "unavailable")]),
    }
}

pub async fn callback(State(state): State<AppState>, Query(q): Query<CallbackQuery>, headers: HeaderMap) -> Response {
    let lang = page_lang(&headers);
    let expired = || {
        error_page(
            &state,
            lang,
            StatusCode::BAD_REQUEST,
            text(
                lang,
                "El inicio de sesión caducó o no empezó desde este navegador. Vuelve a intentarlo desde Warlay.",
                "The sign-in expired or did not start in this browser. Try again from Warlay.",
            ),
        )
    };
    let (Some(nonce), Some(cookie)) = (q.state.as_deref(), cookie_of(&headers)) else {
        return expired();
    };
    if !is_url_safe(nonce, 1, 128) || nonce != cookie {
        return expired();
    }
    let Ok(Some(flow)) = take::<Flow>(&state, &flow_key(nonce)).await else {
        return expired();
    };
    let lang = flow.lang();
    if q.error.is_some() {
        return to_app(&state, &flow, &[("error", "access_denied")]);
    }
    let Some(code) = q.code.as_deref() else {
        return to_app(&state, &flow, &[("error", "provider_invalid")]);
    };
    let identity = match google_identity(&state, code).await {
        Ok(identity) => identity,
        Err(reason) => return to_app(&state, &flow, &[("error", reason)]),
    };
    match resolve(&state, &flow, nonce, identity, lang).await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(?error, "google sign-in failed");
            to_app(&state, &flow, &[("error", "server_error")])
        }
    }
}

/// Known Google identity: sign in. Known e-mail: ask for the password first.
/// Neither: a new account.
async fn resolve(
    state: &AppState,
    flow: &Flow,
    nonce: &str,
    identity: GoogleIdentity,
    lang: Lang,
) -> ApiResult<Response> {
    let known: Option<Uuid> = sqlx::query_scalar(
        "UPDATE external_identities SET last_login_at = now(), email = $3
         WHERE provider = $1 AND subject = $2 RETURNING user_id",
    )
    .bind(PROVIDER)
    .bind(&identity.subject)
    .bind(&identity.email)
    .fetch_optional(&state.db)
    .await?;
    if let Some(user_id) = known {
        return Ok(finish(state, flow, user_id).await);
    }

    let by_email: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(&identity.email)
        .fetch_optional(&state.db)
        .await?;
    if let Some(user_id) = by_email {
        let user = load_user(state, user_id)
            .await?
            .ok_or_else(|| ApiError::internal("user vanished"))?;
        if !user.password_set {
            // Nothing to confirm the link with.
            return Ok(to_app(state, flow, &[("error", "account_conflict")]));
        }
        let link_id = random_token();
        put(
            state,
            &link_key(&link_id),
            &PendingLink {
                flow: flow.clone(),
                nonce: nonce.to_string(),
                subject: identity.subject,
                email: identity.email.clone(),
                user_id,
            },
            FLOW_TTL_SECS,
        )
        .await?;
        // The cookie stays: the form posts back under it.
        let page = html_sign_in_form(
            pages::link_account(base(state), lang, &identity.email, &link_id, None),
            lang,
        );
        return Ok(with_cookie(page, set_cookie(state, nonce, FLOW_TTL_SECS)));
    }

    let user_id = create_account(state, &identity).await?;
    Ok(finish(state, flow, user_id).await)
}

/// `jhon.fernandez+x@gmail.com` -> `jhonfernande`: letters, digits and `_`,
/// at most 14 so a numeric suffix still fits the 20 the API allows.
fn username_base(email: &str) -> String {
    let local = email.split('@').next().unwrap_or_default();
    // Drop a `+tag`: it is how the player files their mail, not who they are.
    let local = local.split('+').next().unwrap_or_default();
    let mut base: String = local
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(14)
        .collect();
    if base.len() < 3 {
        base = format!("{base}player");
    }
    base.to_lowercase()
}

async fn create_account(state: &AppState, identity: &GoogleIdentity) -> ApiResult<Uuid> {
    ensure_hashing_capacity()?;
    let id = Uuid::new_v4();
    // Nobody knows this password: the account has none until its owner sets one.
    let password_hash = hash_password(&random_token()).await?;
    let display_name: String = {
        let name = identity.name.trim();
        if name.is_empty() {
            username_base(&identity.email)
        } else {
            name.chars().take(32).collect()
        }
    };
    let base = username_base(&identity.email);
    for attempt in 0..6 {
        let username = if attempt == 0 {
            base.clone()
        } else {
            format!("{base}{}", &Uuid::new_v4().simple().to_string()[..4])
        };
        let mut tx = state.db.begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, password_set, display_name, avatar_color)
             VALUES ($1, $2, $3, $4, FALSE, $5, $6)",
        )
        .bind(id)
        .bind(&username)
        .bind(&identity.email)
        .bind(&password_hash)
        .bind(&display_name)
        .bind(pick_color(id))
        .execute(&mut *tx)
        .await;
        match inserted {
            Ok(_) => {}
            Err(sqlx::Error::Database(db_error)) if db_error.is_unique_violation() => {
                if db_error.constraint().unwrap_or_default().contains("email") {
                    // Someone registered this e-mail a moment ago.
                    return Err(ApiError::conflict("email_taken", "ese correo ya tiene una cuenta"));
                }
                continue;
            }
            Err(error) => return Err(error.into()),
        }
        link_identity(&mut tx, id, &identity.subject, &identity.email).await?;
        tx.commit().await?;
        return Ok(id);
    }
    Err(ApiError::internal("no free username"))
}

async fn link_identity(
    tx: &mut sqlx::PgConnection,
    user_id: Uuid,
    subject: &str,
    email: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO external_identities (provider, subject, user_id, email) VALUES ($1, $2, $3, $4)")
        .bind(PROVIDER)
        .bind(subject)
        .bind(user_id)
        .bind(email)
        .execute(tx)
        .await?;
    Ok(())
}

// --- Step 2b: confirming the password to link an existing account ---------

#[derive(Debug, Deserialize)]
pub struct LinkForm {
    link: String,
    password: String,
}

pub async fn link(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Form(form): Form<LinkForm>,
) -> Response {
    let lang = page_lang(&headers);
    let expired = || {
        error_page(
            &state,
            lang,
            StatusCode::BAD_REQUEST,
            text(
                lang,
                "La vinculación caducó. Vuelve a intentarlo desde Warlay.",
                "The link request expired. Try again from Warlay.",
            ),
        )
    };
    if !is_url_safe(&form.link, 1, 128) {
        return expired();
    }
    let Ok(Some(pending)) = peek::<PendingLink>(&state, &link_key(&form.link)).await else {
        return expired();
    };
    if cookie_of(&headers).as_deref() != Some(pending.nonce.as_str()) {
        return expired();
    }
    let lang = pending.flow.lang();
    let busy = |status, es, en| error_page(&state, lang, status, text(lang, es, en));

    if ensure_hashing_capacity().is_err() {
        return busy(
            StatusCode::SERVICE_UNAVAILABLE,
            "El servidor está ocupado, inténtalo de nuevo.",
            "The server is busy, try again.",
        );
    }
    let limited = async {
        if let Some(ip) = ip {
            rate_limit(&state, &format!("oauth-link-ip:{ip}"), 30, 900).await?;
        }
        rate_limit(&state, &format!("oauth-link:{}", pending.user_id), 5, 900).await
    }
    .await;
    if limited.is_err() {
        return busy(
            StatusCode::TOO_MANY_REQUESTS,
            "Demasiados intentos, espera unos minutos.",
            "Too many attempts, wait a few minutes.",
        );
    }
    let user = match load_user(&state, pending.user_id).await {
        Ok(Some(user)) => user,
        _ => return expired(),
    };
    let matches = verify_password(&form.password, Some(&user.password_hash))
        .await
        .unwrap_or(false);
    if !matches {
        let mut response = html_sign_in_form(
            pages::link_account(
                base(&state),
                lang,
                &pending.email,
                &form.link,
                Some(text(lang, "Contraseña incorrecta.", "Wrong password.")),
            ),
            lang,
        );
        *response.status_mut() = StatusCode::UNAUTHORIZED;
        return with_cookie(response, set_cookie(&state, &pending.nonce, FLOW_TTL_SECS));
    }
    // Spent only on success, so a typo does not end the flow (the rate limit
    // bounds the guesses).
    if drop_key(&state, &link_key(&form.link)).await.is_err() {
        return expired();
    }
    let linked = async {
        let mut conn = state.db.acquire().await?;
        link_identity(&mut conn, pending.user_id, &pending.subject, &pending.email).await
    }
    .await;
    match linked {
        Ok(()) => finish(&state, &pending.flow, pending.user_id).await,
        // Already linked by a parallel request: the sign-in is still valid.
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            finish(&state, &pending.flow, pending.user_id).await
        }
        Err(error) => {
            tracing::error!(?error, "linking google identity failed");
            to_app(&state, &pending.flow, &[("error", "server_error")])
        }
    }
}

// --- Step 3: the app trades the code for a session -----------------------

#[derive(Debug, Serialize, Deserialize)]
struct CodeRecord {
    user_id: Uuid,
    challenge: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeRequest {
    code: String,
    verifier: String,
    /// For accounts with an authenticator app: the app asks for it when the
    /// first try comes back `totp_required`, and tries again with the same code.
    totp_code: Option<String>,
}

pub async fn exchange(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<ExchangeRequest>,
) -> ApiResult<Json<AuthResponse>> {
    if let Some(ip) = ip {
        rate_limit(&state, &format!("oauth-exchange-ip:{ip}"), 60, 300).await?;
    }
    let invalid = || ApiError::bad_request("invalid_code", "el código de inicio de sesión no es válido o caducó");
    if !is_url_safe(&req.code, 1, 128) || !(43..=128).contains(&req.verifier.len()) {
        return Err(invalid());
    }
    // A wrong verifier burns the code; a missing second step does not, so the
    // app can ask for it and try again with the same code.
    let record: CodeRecord = peek(&state, &code_key(&req.code)).await?.ok_or_else(invalid)?;
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(req.verifier.as_bytes()));
    if !constant_time_eq::constant_time_eq(challenge.as_bytes(), record.challenge.as_bytes()) {
        drop_key(&state, &code_key(&req.code)).await?;
        return Err(invalid());
    }
    crate::routes::totp::require(&state, record.user_id, req.totp_code.as_deref()).await?;
    // Spent here, so two requests cannot both get a session from one code.
    let record: CodeRecord = take(&state, &code_key(&req.code)).await?.ok_or_else(invalid)?;
    let user = load_user(&state, record.user_id).await?.ok_or_else(invalid)?;
    Ok(Json(start_session(&state, user, &headers).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(claims: serde_json::Value) -> String {
        let enc = |v: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v);
        format!("{}.{}.sig", enc(b"{}"), enc(claims.to_string().as_bytes()))
    }

    fn claims() -> serde_json::Value {
        serde_json::json!({
            "iss": "https://accounts.google.com", "aud": "cid", "sub": "123", "exp": 2000,
            "email": "Jhon@Example.com", "email_verified": true, "name": "Jhon"
        })
    }

    #[test]
    fn a_valid_id_token_gives_the_identity() {
        let identity = parse_id_token(&token(claims()), "cid", 1000).unwrap();
        assert_eq!(identity.subject, "123");
        assert_eq!(identity.email, "jhon@example.com");
    }

    #[test]
    fn the_id_token_must_be_for_this_client_unexpired_and_verified() {
        assert!(parse_id_token(&token(claims()), "other", 1000).is_err());
        assert!(parse_id_token(&token(claims()), "cid", 2000).is_err());
        let mut c = claims();
        c["iss"] = "https://evil.example".into();
        assert!(parse_id_token(&token(c), "cid", 1000).is_err());
        let mut c = claims();
        c["email_verified"] = false.into();
        assert_eq!(
            parse_id_token(&token(c), "cid", 1000).unwrap_err(),
            "email_not_verified"
        );
        let mut c = claims();
        c["email_verified"] = "true".into();
        assert!(parse_id_token(&token(c), "cid", 1000).is_ok());
        assert!(parse_id_token("nonsense", "cid", 1000).is_err());
    }

    #[test]
    fn usernames_come_from_the_email() {
        assert_eq!(username_base("Jhon.Fernandez+x@gmail.com"), "jhonfernandez");
        assert_eq!(username_base("a@b.co"), "aplayer");
        assert!(username_base("x@y.com").len() >= 3);
    }

    #[test]
    fn the_loopback_url_only_ever_points_at_this_machine() {
        let flow = Flow {
            port: 51234,
            state: "abc".into(),
            challenge: "c".into(),
            lang: "es".into(),
        };
        assert_eq!(
            flow.loopback(&[("code", "x y")]),
            "http://127.0.0.1:51234/callback?state=abc&code=x+y"
        );
    }

    #[test]
    fn cookies_are_read_by_name() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_static("a=1; wl_oauth=xyz; b=2"));
        assert_eq!(cookie_of(&headers).as_deref(), Some("xyz"));
        assert_eq!(cookie_of(&HeaderMap::new()), None);
    }
}
