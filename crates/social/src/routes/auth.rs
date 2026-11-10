use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{
    ensure_hashing_capacity, hash_password, issue_access_token, random_token, rate_limit, revoke_access_token,
    revoke_user_access_tokens, token_hash, verify_password, AuthUser, ClientIp,
};
use crate::error::{ApiError, ApiResult};
use crate::models::{load_user, pick_color, Me, UserRow};
use crate::realtime::broadcast_presence;
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub user: Me,
    pub tokens: Tokens,
    /// Recovery codes, only in the answer to sign-up and shown only once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_codes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    /// Username or email.
    pub login: String,
    pub password: String,
    /// The second step, for accounts that turned on an authenticator app: a
    /// code from it, or a recovery code.
    pub totp_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMeRequest {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_color: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordRequest {
    /// Required to change a password, not to set the first one on an account
    /// that was created with Google.
    pub current_password: Option<String>,
    pub new_password: String,
}

pub fn validate_username(username: &str) -> ApiResult<()> {
    let ok = (3..=20).contains(&username.len()) && username.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ok {
        return Err(ApiError::bad_request(
            "invalid_username",
            "el usuario debe tener de 3 a 20 caracteres: letras, números o _",
        ));
    }
    Ok(())
}

fn validate_email(email: &str) -> ApiResult<()> {
    let ok = email.len() <= 254
        && email
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.') && !domain.starts_with('.'));
    if !ok {
        return Err(ApiError::bad_request("invalid_email", "el correo no es válido"));
    }
    Ok(())
}

pub(crate) fn validate_password(password: &str) -> ApiResult<()> {
    if password.chars().count() < 8 || password.len() > 128 {
        return Err(ApiError::bad_request(
            "weak_password",
            "la contraseña debe tener al menos 8 caracteres",
        ));
    }
    Ok(())
}

pub fn validate_display_name(name: &str) -> ApiResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 32 {
        return Err(ApiError::bad_request(
            "invalid_display_name",
            "el nombre visible debe tener entre 1 y 32 caracteres",
        ));
    }
    Ok(name.to_string())
}

fn user_agent(headers: &HeaderMap) -> String {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .chars()
        .take(200)
        .collect()
}

/// Creates a session row and the token pair for it.
pub(crate) async fn start_session(state: &AppState, user: UserRow, headers: &HeaderMap) -> ApiResult<AuthResponse> {
    let session_id = Uuid::new_v4();
    let refresh_token = random_token();
    let expires_at = Utc::now() + Duration::days(state.settings.refresh_token_ttl_days);
    sqlx::query(
        "INSERT INTO sessions (id, user_id, refresh_token_hash, user_agent, expires_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(session_id)
    .bind(user.id)
    .bind(token_hash(&refresh_token))
    .bind(user_agent(headers))
    .bind(expires_at)
    .execute(&state.db)
    .await?;
    let access_token = issue_access_token(state, user.id, session_id).await?;
    Ok(AuthResponse {
        user: user.into(),
        tokens: Tokens {
            access_token,
            refresh_token,
            expires_in: state.settings.access_token_ttl_secs,
        },
        recovery_codes: None,
    })
}

pub async fn register(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let username = req.username.trim().to_string();
    let email = req.email.trim().to_lowercase();
    validate_username(&username)?;
    validate_email(&email)?;
    validate_password(&req.password)?;
    let display_name = validate_display_name(req.display_name.as_deref().unwrap_or(&username))?;
    ensure_hashing_capacity()?;
    if let Some(ip) = ip {
        rate_limit(&state, &format!("register-ip:{ip}"), 30, 3600).await?;
    }
    rate_limit(&state, &format!("register:{}", email), 5, 3600).await?;

    let id = Uuid::new_v4();
    let password_hash = hash_password(&req.password).await?;
    let inserted = sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, avatar_color)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(&username)
    .bind(&email)
    .bind(&password_hash)
    .bind(&display_name)
    .bind(pick_color(id))
    .execute(&state.db)
    .await;
    if let Err(sqlx::Error::Database(db_error)) = &inserted {
        if db_error.is_unique_violation() {
            let taken = db_error.constraint().unwrap_or_default();
            return Err(if taken.contains("email") {
                ApiError::conflict("email_taken", "ese correo ya tiene una cuenta")
            } else {
                ApiError::conflict("username_taken", "ese nombre de usuario ya está en uso")
            });
        }
    }
    inserted?;

    let user = load_user(&state, id)
        .await?
        .ok_or_else(|| ApiError::internal("user vanished"))?;
    let mut response = start_session(&state, user, &headers).await?;
    // A new account starts with recovery codes; this is the only time they are
    // shown without asking for the password.
    response.recovery_codes = Some(super::recovery::create_set(&state, id).await?);
    Ok(Json(response))
}

pub async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let login = req.login.trim().to_lowercase();
    ensure_hashing_capacity()?;
    // Per account (guessing one password) and per address (trying one
    // password against many accounts). The address limits are generous: many
    // players share one address (a LAN party, a household, carrier NAT).
    if let Some(ip) = ip {
        rate_limit(&state, &format!("login-ip:{ip}"), 100, 300).await?;
    }
    rate_limit(&state, &format!("login:{login}"), 10, 300).await?;
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, username::text AS username, email::text AS email, password_hash, password_set,
                EXISTS (SELECT 1 FROM user_totp t WHERE t.user_id = users.id AND t.enabled_at IS NOT NULL) AS totp_enabled,
                display_name, avatar_color, bio, created_at
         FROM users WHERE username = $1 OR email = $1",
    )
    .bind(&login)
    .fetch_optional(&state.db)
    .await?;
    // Same error either way: don't reveal which accounts exist.
    let invalid = || {
        ApiError::new(
            axum::http::StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "usuario o contraseña incorrectos",
        )
    };
    // Hashes even for unknown accounts, so the response time doesn't tell
    // them apart either.
    let matches = verify_password(&req.password, user.as_ref().map(|u| u.password_hash.as_str())).await?;
    // An account born with Google has no password yet: nothing can match.
    let user = user.filter(|u| matches && u.password_set).ok_or_else(invalid)?;
    // Only after the password is right, so this does not say which accounts use it.
    if user.totp_enabled {
        super::totp::require(&state, user.id, req.totp_code.as_deref()).await?;
    }
    Ok(Json(start_session(&state, user, &headers).await?))
}

/// Rotates the refresh token and issues a fresh access token.
pub async fn refresh(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<RefreshRequest>,
) -> ApiResult<Json<Tokens>> {
    if let Some(ip) = ip {
        rate_limit(&state, &format!("refresh-ip:{ip}"), 600, 300).await?;
    }
    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, user_id FROM sessions
         WHERE refresh_token_hash = $1 AND revoked_at IS NULL AND expires_at > now()",
    )
    .bind(token_hash(&req.refresh_token))
    .fetch_optional(&state.db)
    .await?;
    let (session_id, user_id) = row.ok_or_else(ApiError::unauthorized)?;
    let refresh_token = random_token();
    sqlx::query("UPDATE sessions SET refresh_token_hash = $1 WHERE id = $2")
        .bind(token_hash(&refresh_token))
        .bind(session_id)
        .execute(&state.db)
        .await?;
    let access_token = issue_access_token(&state, user_id, session_id).await?;
    Ok(Json(Tokens {
        access_token,
        refresh_token,
        expires_in: state.settings.access_token_ttl_secs,
    }))
}

pub async fn logout(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1")
        .bind(auth.session_id)
        .execute(&state.db)
        .await?;
    revoke_access_token(&state, &auth.token).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn me(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Me>> {
    let user = load_user(&state, auth.user_id)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    Ok(Json(user.into()))
}

pub async fn update_me(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<UpdateMeRequest>,
) -> ApiResult<Json<Me>> {
    crate::chat::invalidate_user(&state, auth.user_id).await;
    if let Some(name) = &req.display_name {
        let name = validate_display_name(name)?;
        sqlx::query("UPDATE users SET display_name = $1 WHERE id = $2")
            .bind(name)
            .bind(auth.user_id)
            .execute(&state.db)
            .await?;
    }
    if let Some(bio) = &req.bio {
        if bio.chars().count() > 190 {
            return Err(ApiError::bad_request(
                "bio_too_long",
                "la biografía admite hasta 190 caracteres",
            ));
        }
        sqlx::query("UPDATE users SET bio = $1 WHERE id = $2")
            .bind(bio.trim())
            .bind(auth.user_id)
            .execute(&state.db)
            .await?;
    }
    if let Some(color) = &req.avatar_color {
        if !crate::models::AVATAR_COLORS.contains(&color.as_str()) {
            return Err(ApiError::bad_request("invalid_color", "color no disponible"));
        }
        sqlx::query("UPDATE users SET avatar_color = $1 WHERE id = $2")
            .bind(color)
            .bind(auth.user_id)
            .execute(&state.db)
            .await?;
    }
    let user = load_user(&state, auth.user_id)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    let _ = broadcast_presence(&state, auth.user_id).await;
    Ok(Json(user.into()))
}

pub async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<ChangePasswordRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let user = load_user(&state, auth.user_id)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    if user.password_set {
        let current = req.current_password.as_deref().unwrap_or_default();
        if !verify_password(current, Some(&user.password_hash)).await? {
            return Err(ApiError::bad_request(
                "invalid_credentials",
                "la contraseña actual no es correcta",
            ));
        }
    }
    validate_password(&req.new_password)?;
    sqlx::query("UPDATE users SET password_hash = $1, password_set = TRUE WHERE id = $2")
        .bind(hash_password(&req.new_password).await?)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;
    // Every other session must log in again.
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND id <> $2 AND revoked_at IS NULL")
        .bind(auth.user_id)
        .bind(auth.session_id)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAccountRequest {
    /// Asked only of accounts that have one (not of those born with Google).
    pub password: Option<String>,
    /// The account's username, typed to confirm.
    pub confirm: String,
}

/// Deletes the account and everything that hangs from it: friendships,
/// messages, memberships, notifications, keys, sessions and its Google link.
/// A room the player hosts is closed first, so nobody is left in a room with no
/// host; communities they own go with them.
pub async fn delete_me(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<DeleteAccountRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_hashing_capacity()?;
    rate_limit(&state, &format!("delete-account:{}", auth.user_id), 5, 3600).await?;
    let user = load_user(&state, auth.user_id)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    if req.confirm.trim().to_lowercase() != user.username.to_lowercase() {
        return Err(ApiError::bad_request(
            "confirmation_mismatch",
            "escribe tu nombre de usuario para confirmar",
        ));
    }
    if user.password_set {
        let password = req.password.as_deref().unwrap_or_default();
        if !verify_password(password, Some(&user.password_hash)).await? {
            return Err(ApiError::bad_request(
                "invalid_credentials",
                "la contraseña no es correcta",
            ));
        }
    }
    super::rooms::leave_current_room(&state, user.id).await?;
    revoke_user_access_tokens(&state, user.id).await?;
    crate::chat::invalidate_user(&state, user.id).await;
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user.id)
        .execute(&state.db)
        .await?;
    tracing::info!(user = %user.id, "account deleted by its owner");
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_usernames_emails_and_passwords() {
        assert!(validate_username("jhon_99").is_ok());
        assert!(validate_username("ab").is_err());
        assert!(validate_username("con espacio").is_err());
        assert!(validate_email("a@b.co").is_ok());
        assert!(validate_email("nope").is_err());
        assert!(validate_email("a@.com").is_err());
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password("1234567").is_err());
        assert!(validate_display_name("   ").is_err());
        assert_eq!(validate_display_name("  Jhon ").unwrap(), "Jhon");
    }
}
