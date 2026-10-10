use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{
    hash_password, issue_access_token, random_token, rate_limit, revoke_access_token, token_hash, verify_password,
    AuthUser,
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
    pub current_password: String,
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

fn validate_password(password: &str) -> ApiResult<()> {
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
async fn start_session(state: &AppState, user: UserRow, headers: &HeaderMap) -> ApiResult<AuthResponse> {
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
    })
}

pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let username = req.username.trim().to_string();
    let email = req.email.trim().to_lowercase();
    validate_username(&username)?;
    validate_email(&email)?;
    validate_password(&req.password)?;
    let display_name = validate_display_name(req.display_name.as_deref().unwrap_or(&username))?;
    rate_limit(&state, &format!("register:{}", email), 5, 3600).await?;

    let id = Uuid::new_v4();
    let password_hash = hash_password(&req.password)?;
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
    Ok(Json(start_session(&state, user, &headers).await?))
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let login = req.login.trim().to_lowercase();
    rate_limit(&state, &format!("login:{login}"), 10, 300).await?;
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, username::text AS username, email::text AS email, password_hash, display_name, avatar_color, bio, created_at
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
    let user = user.ok_or_else(invalid)?;
    if !verify_password(&req.password, &user.password_hash) {
        return Err(invalid());
    }
    Ok(Json(start_session(&state, user, &headers).await?))
}

/// Rotates the refresh token and issues a fresh access token.
pub async fn refresh(State(state): State<AppState>, Json(req): Json<RefreshRequest>) -> ApiResult<Json<Tokens>> {
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
    if !verify_password(&req.current_password, &user.password_hash) {
        return Err(ApiError::bad_request(
            "invalid_credentials",
            "la contraseña actual no es correcta",
        ));
    }
    validate_password(&req.new_password)?;
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(hash_password(&req.new_password)?)
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
