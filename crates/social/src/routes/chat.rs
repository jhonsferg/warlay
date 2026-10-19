use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::chat;
use crate::error::ApiResult;
use crate::models::Message;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct HistoryQuery {
    before: Option<DateTime<Utc>>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessage {
    body: String,
    /// The client's id for this send, shared with its realtime-socket
    /// attempt so a retry never posts the message twice.
    request_id: Option<String>,
}

/// `GET /chat/:kind/:id/messages` -- newest last, paginated with `before`.
/// Served by the main server and by every helper.
pub async fn history(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((kind, id)): Path<(String, Uuid)>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<Vec<Message>>> {
    Ok(Json(
        chat::history(&state, auth.user_id, &kind, id, query.before, query.limit.unwrap_or(50)).await?,
    ))
}

pub async fn send(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((kind, id)): Path<(String, Uuid)>,
    Json(req): Json<SendMessage>,
) -> ApiResult<Json<Message>> {
    Ok(Json(
        chat::send(&state, auth.user_id, &kind, id, &req.body, req.request_id.as_deref()).await?,
    ))
}
