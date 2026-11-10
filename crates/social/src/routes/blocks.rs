//! Blocking and reporting other players.
//!
//! Blocking ends the friendship and any pending request or invitation between
//! the two, and keeps them from sending new ones (friend requests, invitations,
//! direct messages, which need a friendship). The blocked player is not told:
//! to them the other one simply can no longer be found or asked. Room and
//! community chat is shared, so the app hides a blocked player's messages there.

use axum::extract::{Path, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{rate_limit, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::models::{load_summaries, load_user, UserSummary};
use crate::realtime::Event;
use crate::state::AppState;

/// Why a player is reported. The app offers exactly these.
const REASONS: &[&str] = &[
    "spam",
    "harassment",
    "inappropriate",
    "cheating",
    "impersonation",
    "other",
];
const MAX_DETAILS: usize = 500;

/// Whether either of the two has blocked the other.
pub async fn blocked_between(state: &AppState, a: Uuid, b: Uuid) -> ApiResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_blocks
                        WHERE (blocker_id = $1 AND blocked_id = $2) OR (blocker_id = $2 AND blocked_id = $1))",
    )
    .bind(a)
    .bind(b)
    .fetch_one(&state.db)
    .await?)
}

/// Whether `blocker` has blocked `blocked` (the other direction does not count).
pub async fn has_blocked(state: &AppState, blocker: Uuid, blocked: Uuid) -> ApiResult<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM user_blocks WHERE blocker_id = $1 AND blocked_id = $2)")
            .bind(blocker)
            .bind(blocked)
            .fetch_one(&state.db)
            .await?,
    )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedUser {
    #[serde(flatten)]
    pub user: UserSummary,
    pub blocked_at: DateTime<Utc>,
}

/// The players the caller has blocked, most recent first.
pub async fn list(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<BlockedUser>>> {
    let rows: Vec<(Uuid, DateTime<Utc>)> =
        sqlx::query_as("SELECT blocked_id, created_at FROM user_blocks WHERE blocker_id = $1 ORDER BY created_at DESC")
            .bind(auth.user_id)
            .fetch_all(&state.db)
            .await?;
    let ids: Vec<Uuid> = rows.iter().map(|(id, _)| *id).collect();
    let users = load_summaries(&state, &ids).await?;
    Ok(Json(
        rows.into_iter()
            .filter_map(|(id, blocked_at)| {
                users
                    .iter()
                    .find(|u| u.id == id)
                    .cloned()
                    .map(|user| BlockedUser { user, blocked_at })
            })
            .collect(),
    ))
}

pub async fn block(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    if target == auth.user_id {
        return Err(ApiError::bad_request("self_block", "no puedes bloquearte a ti mismo"));
    }
    load_user(&state, target)
        .await?
        .ok_or_else(|| ApiError::not_found("usuario no encontrado"))?;
    rate_limit(&state, &format!("block:{}", auth.user_id), 60, 3600).await?;

    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO user_blocks (blocker_id, blocked_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(auth.user_id)
        .bind(target)
        .execute(&mut *tx)
        .await?;
    let was_friends = sqlx::query("DELETE FROM friendships WHERE user_a = $1 AND user_b = $2")
        .bind(auth.user_id.min(target))
        .bind(auth.user_id.max(target))
        .execute(&mut *tx)
        .await?
        .rows_affected()
        > 0;
    let requests: Vec<Uuid> = sqlx::query_scalar(
        "DELETE FROM friend_requests
         WHERE (requester_id = $1 AND addressee_id = $2) OR (requester_id = $2 AND addressee_id = $1)
         RETURNING id",
    )
    .bind(auth.user_id)
    .bind(target)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM invites WHERE status = 'pending'
         AND ((inviter_id = $1 AND invitee_id = $2) OR (inviter_id = $2 AND invitee_id = $1))",
    )
    .bind(auth.user_id)
    .bind(target)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    crate::chat::invalidate(&state, &crate::chat::dm_key(auth.user_id, target)).await;
    // Both screens drop what no longer exists.
    if was_friends {
        state
            .realtime
            .emit(&[auth.user_id], &Event::FriendRemoved { user_id: target })
            .await;
        state
            .realtime
            .emit(&[target], &Event::FriendRemoved { user_id: auth.user_id })
            .await;
    }
    for id in requests {
        state
            .realtime
            .emit(
                &[auth.user_id, target],
                &Event::FriendRequest {
                    request: serde_json::json!({ "id": id, "removed": true }),
                },
            )
            .await;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn unblock(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query("DELETE FROM user_blocks WHERE blocker_id = $1 AND blocked_id = $2")
        .bind(auth.user_id)
        .bind(target)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequest {
    pub user_id: Uuid,
    pub reason: String,
    pub details: Option<String>,
    /// A message the report is about, if any (kept as context for the review).
    pub message_id: Option<Uuid>,
}

/// Reports a player. Reports are stored for whoever runs the server to review
/// (`SELECT * FROM user_reports WHERE status = 'open'`); the reporter is not
/// shown an outcome. Reporting the same player again within a day is accepted
/// and not stored twice.
pub async fn report(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<ReportRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if req.user_id == auth.user_id {
        return Err(ApiError::bad_request("self_report", "no puedes reportarte a ti mismo"));
    }
    if !REASONS.contains(&req.reason.as_str()) {
        return Err(ApiError::bad_request("invalid_reason", "motivo de reporte no válido"));
    }
    let details = req.details.as_deref().map(str::trim).unwrap_or_default();
    if details.chars().count() > MAX_DETAILS {
        return Err(ApiError::bad_request(
            "details_too_long",
            "los detalles admiten hasta 500 caracteres",
        ));
    }
    load_user(&state, req.user_id)
        .await?
        .ok_or_else(|| ApiError::not_found("usuario no encontrado"))?;
    rate_limit(&state, &format!("report:{}", auth.user_id), 10, 3600).await?;

    let duplicate: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_reports
                        WHERE reporter_id = $1 AND reported_id = $2 AND status = 'open'
                          AND created_at > now() - interval '1 day')",
    )
    .bind(auth.user_id)
    .bind(req.user_id)
    .fetch_one(&state.db)
    .await?;
    if !duplicate {
        sqlx::query(
            "INSERT INTO user_reports (id, reporter_id, reported_id, reason, details, message_id)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(auth.user_id)
        .bind(req.user_id)
        .bind(&req.reason)
        .bind(details)
        .bind(req.message_id)
        .execute(&state.db)
        .await?;
        tracing::info!(reporter = %auth.user_id, reported = %req.user_id, reason = %req.reason, "player reported");
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
