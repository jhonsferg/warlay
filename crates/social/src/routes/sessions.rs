//! The devices signed in to an account, and ending them from another one.

use axum::extract::{Path, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::{revoke_session_access_tokens, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub id: Uuid,
    /// The browser or program that signed in, as it introduced itself.
    pub user_agent: String,
    /// How it signed in: `password` or `google`.
    pub method: String,
    pub created_at: DateTime<Utc>,
    /// The last time it renewed its access (every few minutes while in use).
    pub last_used_at: DateTime<Utc>,
    /// The session making this request.
    #[sqlx(default)]
    pub current: bool,
}

/// The account's open sessions, most recently used first.
pub async fn list(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<SessionView>>> {
    let mut sessions = sqlx::query_as::<_, SessionView>(
        "SELECT id, user_agent, method, created_at, last_used_at, FALSE AS current
         FROM sessions
         WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > now()
         ORDER BY last_used_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    for session in &mut sessions {
        session.current = session.id == auth.session_id;
    }
    Ok(Json(sessions))
}

/// Ends one session: its refresh token stops working and its access tokens are
/// cut right away. Ending the current one is the same as signing out.
pub async fn revoke(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let ended: Option<Uuid> = sqlx::query_scalar(
        "UPDATE sessions SET revoked_at = now() WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL RETURNING id",
    )
    .bind(id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?;
    let id = ended.ok_or_else(|| ApiError::not_found("esa sesión no existe o ya terminó"))?;
    revoke_session_access_tokens(&state, id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Ends every session but the current one.
pub async fn revoke_others(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<serde_json::Value>> {
    let ended: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE sessions SET revoked_at = now()
         WHERE user_id = $1 AND id <> $2 AND revoked_at IS NULL RETURNING id",
    )
    .bind(auth.user_id)
    .bind(auth.session_id)
    .fetch_all(&state.db)
    .await?;
    for id in &ended {
        revoke_session_access_tokens(&state, *id).await?;
    }
    Ok(Json(serde_json::json!({ "revoked": ended.len() })))
}
