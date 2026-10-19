use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::chat;
use crate::error::ApiResult;
use crate::models::Message;
use crate::receipts::{self, Receipt, ReceiptState, Unread};
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkReceipt {
    state: ReceiptState,
    /// Time of the newest message received/read.
    up_to: DateTime<Utc>,
}

/// `POST /chat/dm/:id/receipt` -- I received/read the conversation up to a time.
pub async fn mark_receipt(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((kind, id)): Path<(String, Uuid)>,
    Json(req): Json<MarkReceipt>,
) -> ApiResult<Json<Receipt>> {
    Ok(Json(
        receipts::mark(&state, auth.user_id, &kind, id, req.state, req.up_to).await?,
    ))
}

/// `GET /chat/dm/:id/receipt` -- how far the other side has received and read.
pub async fn peer_receipt(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((kind, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<Receipt>> {
    Ok(Json(receipts::peer_receipt(&state, auth.user_id, &kind, id).await?))
}

/// `GET /chat/unread` -- unread direct messages per friend.
pub async fn unread(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<Unread>>> {
    Ok(Json(receipts::unread(&state, auth.user_id).await?))
}

/// `DELETE /chat/:kind/:id/messages/:message_id` -- sender only.
pub async fn delete_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((kind, id, message_id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    chat::delete_message(&state, auth.user_id, &kind, id, message_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
