//! Account recovery without e-mail: one-time recovery codes.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{
    ensure_hashing_capacity, hash_password, rate_limit, revoke_user_access_tokens, verify_password, AuthUser, ClientIp,
};
use crate::error::{ApiError, ApiResult};
use crate::models::load_user;
use crate::recovery;
use crate::routes::auth::validate_password;
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Codes that have not been used yet.
    pub remaining: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegenerateRequest {
    /// The current password: new codes replace the old ones, so it is asked
    /// again.
    pub password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Codes {
    /// Shown only now: the server keeps just their hashes.
    pub codes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverRequest {
    /// Username or e-mail.
    pub login: String,
    pub code: String,
    pub new_password: String,
}

/// Creates a fresh set of codes for `user_id`, replacing any older ones, and
/// returns them (in the form shown to the player).
pub async fn create_set(state: &AppState, user_id: Uuid) -> ApiResult<Vec<String>> {
    let codes: Vec<String> = (0..recovery::CODES_PER_SET).map(|_| recovery::generate()).collect();
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM recovery_codes WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    for code in &codes {
        let canonical = recovery::normalize(code).ok_or_else(|| ApiError::internal("generated an invalid code"))?;
        sqlx::query("INSERT INTO recovery_codes (user_id, code_hash) VALUES ($1, $2)")
            .bind(user_id)
            .bind(recovery::hash(&canonical))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(codes)
}

/// How many unused codes the signed-in player still has.
pub async fn status(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Status>> {
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM recovery_codes WHERE user_id = $1 AND used_at IS NULL")
            .bind(auth.user_id)
            .fetch_one(&state.db)
            .await?;
    Ok(Json(Status { remaining }))
}

/// Replaces the player's codes with a new set.
pub async fn regenerate(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<RegenerateRequest>,
) -> ApiResult<Json<Codes>> {
    ensure_hashing_capacity()?;
    rate_limit(&state, &format!("recovery-gen:{}", auth.user_id), 5, 3600).await?;
    let user = load_user(&state, auth.user_id)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    if !user.password_set {
        return Err(ApiError::bad_request(
            "password_not_set",
            "primero crea una contraseña para tu cuenta",
        ));
    }
    if !verify_password(&req.password, Some(&user.password_hash)).await? {
        return Err(ApiError::bad_request(
            "invalid_credentials",
            "la contraseña no es correcta",
        ));
    }
    Ok(Json(Codes {
        codes: create_set(&state, auth.user_id).await?,
    }))
}

/// Sets a new password with a recovery code. Every session of the account is
/// closed, and the code is spent.
pub async fn recover(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<RecoverRequest>,
) -> ApiResult<Json<Status>> {
    // Checked before spending the code, so a weak password does not burn it.
    validate_password(&req.new_password)?;
    ensure_hashing_capacity()?;
    let login = req.login.trim().to_lowercase();
    if let Some(ip) = ip {
        rate_limit(&state, &format!("recover-ip:{ip}"), 20, 3600).await?;
    }
    rate_limit(&state, &format!("recover:{login}"), 5, 900).await?;

    // The same answer whether the account or the code is wrong.
    let invalid = || {
        ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "invalid_recovery",
            "usuario o código de recuperación incorrectos",
        )
    };
    let code = recovery::normalize(&req.code).ok_or_else(invalid)?;

    let mut tx = state.db.begin().await?;
    // One statement finds the account, checks the code belongs to it and
    // spends it, so two requests cannot use the same code.
    let user_id: Option<Uuid> = sqlx::query_scalar(
        "UPDATE recovery_codes SET used_at = now()
         WHERE code_hash = $1 AND used_at IS NULL
           AND user_id = (SELECT id FROM users WHERE username = $2 OR email = $2)
         RETURNING user_id",
    )
    .bind(recovery::hash(&code))
    .bind(&login)
    .fetch_optional(&mut *tx)
    .await?;
    let user_id = user_id.ok_or_else(invalid)?;

    sqlx::query("UPDATE users SET password_hash = $1, password_set = TRUE WHERE id = $2")
        .bind(hash_password(&req.new_password).await?)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM recovery_codes WHERE user_id = $1 AND used_at IS NULL")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
    tx.commit().await?;

    // Access tokens live in Redis for up to their TTL: cut them now too.
    revoke_user_access_tokens(&state, user_id).await?;
    Ok(Json(Status { remaining }))
}
