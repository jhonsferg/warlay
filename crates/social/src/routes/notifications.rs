use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::ApiResult;
use crate::models::{Notification, NotificationRow};
use crate::realtime::Event;
use crate::state::AppState;

/// Persists a notification for `user_id` and pushes it live.
pub async fn notify(state: &AppState, user_id: Uuid, kind: &str, payload: serde_json::Value) -> ApiResult<()> {
    let row = sqlx::query_as::<_, NotificationRow>(
        "INSERT INTO notifications (id, user_id, kind, payload) VALUES ($1, $2, $3, $4)
         RETURNING id, kind, payload, read_at, created_at",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(kind)
    .bind(payload)
    .fetch_one(&state.db)
    .await?;
    state
        .realtime
        .emit(
            &[user_id],
            &Event::Notification {
                notification: row.into(),
            },
        )
        .await;
    Ok(())
}

pub async fn list(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<Notification>>> {
    let rows = sqlx::query_as::<_, NotificationRow>(
        "SELECT id, kind, payload, read_at, created_at FROM notifications
         WHERE user_id = $1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

pub async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query("UPDATE notifications SET read_at = now() WHERE id = $1 AND user_id = $2 AND read_at IS NULL")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn mark_all_read(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query("UPDATE notifications SET read_at = now() WHERE user_id = $1 AND read_at IS NULL")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
