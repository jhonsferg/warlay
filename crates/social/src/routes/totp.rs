//! Two-step sign-in with an authenticator app (TOTP).
//!
//! Turning it on is two calls: `setup` makes a secret (not active yet) and
//! `enable` activates it once the player proves their app shows the right code.
//! From then on password sign-in, and Google sign-in, also ask for a code. A
//! recovery code works in place of a code from the app, for a lost phone.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{ensure_hashing_capacity, rate_limit, verify_password, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::models::{load_user, UserRow};
use crate::recovery;
use crate::state::AppState;
use crate::totp;

const ISSUER: &str = "Warlay";

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
    /// The secret as text, to type into an app that cannot scan.
    pub secret: String,
    /// What the QR code holds.
    pub otpauth_uri: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupRequest {
    /// Asked of accounts that have a password.
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableRequest {
    pub code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisableRequest {
    pub password: Option<String>,
    /// A code from the app, or a recovery code.
    pub code: String,
}

// 403, not 401: the account is known and the session is fine, so the client must
// not treat it as an expired session.
fn required() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "totp_required",
        "escribe el código de tu aplicación de autenticación",
    )
}

fn invalid() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "invalid_totp",
        "el código no es correcto o ya se usó",
    )
}

async fn check_password(user: &UserRow, password: Option<&str>) -> ApiResult<()> {
    if user.password_set && !verify_password(password.unwrap_or_default(), Some(&user.password_hash)).await? {
        return Err(ApiError::bad_request(
            "invalid_credentials",
            "la contraseña no es correcta",
        ));
    }
    Ok(())
}

/// The account's secret, if it has TOTP turned on: `(secret, last used step)`.
async fn active_secret(state: &AppState, user_id: Uuid) -> ApiResult<Option<(Vec<u8>, u64)>> {
    let row: Option<(Vec<u8>, i64)> =
        sqlx::query_as("SELECT secret, last_step FROM user_totp WHERE user_id = $1 AND enabled_at IS NOT NULL")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await?;
    let Some((sealed, last_step)) = row else {
        return Ok(None);
    };
    let secret = totp::open(&state.settings.helper_secret, user_id, &sealed)
        .ok_or_else(|| ApiError::internal("a TOTP secret does not open"))?;
    Ok(Some((secret, last_step.max(0) as u64)))
}

/// Whether sign-in to this account asks for a second step.
pub async fn is_enabled(state: &AppState, user_id: Uuid) -> ApiResult<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM user_totp WHERE user_id = $1 AND enabled_at IS NOT NULL)")
            .bind(user_id)
            .fetch_one(&state.db)
            .await?,
    )
}

/// Checks the second step of a sign-in. Does nothing for accounts without
/// TOTP; for the others it needs `code`: a code from the app (each works once),
/// or one of the account's recovery codes. Failures are limited per account.
pub async fn require(state: &AppState, user_id: Uuid, code: Option<&str>) -> ApiResult<()> {
    let Some((secret, last_step)) = active_secret(state, user_id).await? else {
        return Ok(());
    };
    let code = code.map(str::trim).filter(|c| !c.is_empty()).ok_or_else(required)?;
    rate_limit(state, &format!("totp:{user_id}"), 10, 300).await?;
    if let Some(step) = totp::verify(&secret, code, now_secs(), last_step) {
        // Moves the "last used" mark forward in the same statement, so two
        // requests with the same code cannot both pass.
        let moved = sqlx::query("UPDATE user_totp SET last_step = $2 WHERE user_id = $1 AND last_step < $2")
            .bind(user_id)
            .bind(step as i64)
            .execute(&state.db)
            .await?;
        return if moved.rows_affected() == 1 {
            Ok(())
        } else {
            Err(invalid())
        };
    }
    if let Some(recovery_code) = recovery::normalize(code) {
        let spent = sqlx::query(
            "UPDATE recovery_codes SET used_at = now() WHERE user_id = $1 AND code_hash = $2 AND used_at IS NULL",
        )
        .bind(user_id)
        .bind(recovery::hash(&recovery_code))
        .execute(&state.db)
        .await?;
        if spent.rows_affected() == 1 {
            return Ok(());
        }
    }
    Err(invalid())
}

pub async fn status(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Status>> {
    Ok(Json(Status {
        enabled: is_enabled(&state, auth.user_id).await?,
    }))
}

/// Makes a secret for the player to add to their app. It does nothing until
/// `enable` confirms it; calling this again replaces an unconfirmed secret.
pub async fn setup(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<SetupRequest>,
) -> ApiResult<Json<Setup>> {
    ensure_hashing_capacity()?;
    rate_limit(&state, &format!("totp-setup:{}", auth.user_id), 10, 3600).await?;
    let user = load_user(&state, auth.user_id)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    if is_enabled(&state, user.id).await? {
        return Err(ApiError::conflict(
            "totp_already_enabled",
            "la verificación en dos pasos ya está activa",
        ));
    }
    check_password(&user, req.password.as_deref()).await?;
    let secret = totp::generate_secret();
    sqlx::query(
        "INSERT INTO user_totp (user_id, secret) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET secret = EXCLUDED.secret, enabled_at = NULL, last_step = 0",
    )
    .bind(user.id)
    .bind(totp::seal(&state.settings.helper_secret, user.id, &secret))
    .execute(&state.db)
    .await?;
    Ok(Json(Setup {
        secret: totp::base32(&secret),
        otpauth_uri: totp::otpauth_uri(ISSUER, &user.username, &secret),
    }))
}

/// Turns it on, once the app shows a valid code for the secret from `setup`.
pub async fn enable(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<EnableRequest>,
) -> ApiResult<Json<Status>> {
    rate_limit(&state, &format!("totp-enable:{}", auth.user_id), 10, 900).await?;
    let row: Option<(Vec<u8>,)> =
        sqlx::query_as("SELECT secret FROM user_totp WHERE user_id = $1 AND enabled_at IS NULL")
            .bind(auth.user_id)
            .fetch_optional(&state.db)
            .await?;
    let (sealed,) = row.ok_or_else(|| ApiError::bad_request("totp_not_started", "primero genera un secreto"))?;
    let secret = totp::open(&state.settings.helper_secret, auth.user_id, &sealed)
        .ok_or_else(|| ApiError::internal("a TOTP secret does not open"))?;
    let step = totp::verify(&secret, &req.code, now_secs(), 0).ok_or_else(invalid)?;
    sqlx::query("UPDATE user_totp SET enabled_at = now(), last_step = $2 WHERE user_id = $1 AND enabled_at IS NULL")
        .bind(auth.user_id)
        .bind(step as i64)
        .execute(&state.db)
        .await?;
    Ok(Json(Status { enabled: true }))
}

/// Turns it off. Asks for the password (if the account has one) and for a code,
/// so a stolen session alone cannot remove the second step.
pub async fn disable(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<DisableRequest>,
) -> ApiResult<Json<Status>> {
    ensure_hashing_capacity()?;
    let user = load_user(&state, auth.user_id)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    check_password(&user, req.password.as_deref()).await?;
    require(&state, user.id, Some(&req.code)).await?;
    sqlx::query("DELETE FROM user_totp WHERE user_id = $1")
        .bind(user.id)
        .execute(&state.db)
        .await?;
    Ok(Json(Status { enabled: false }))
}
