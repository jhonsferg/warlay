//! Delivery and read state of direct messages, and unread counts.
//!
//! Each user keeps one cursor pair per DM channel: everything up to
//! `delivered_at` reached one of their devices, everything up to `read_at`
//! was seen. A sender derives a message's state ("sent", "delivered",
//! "read") by comparing its timestamp with the peer's cursors, so no
//! per-message rows are written when a conversation is read.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::rate_limit;
use crate::chat::{authorize, dm_key};
use crate::error::{ApiError, ApiResult};
use crate::models::friend_ids;
use crate::realtime::Event;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub delivered_at: DateTime<Utc>,
    pub read_at: DateTime<Utc>,
}

impl Receipt {
    fn none() -> Self {
        Self {
            delivered_at: DateTime::UNIX_EPOCH,
            read_at: DateTime::UNIX_EPOCH,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReceiptState {
    Delivered,
    Read,
}

/// A conversation with messages the user hasn't read.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Unread {
    pub peer_id: Uuid,
    pub count: i64,
    /// Time of the newest unread message: what to acknowledge as delivered.
    pub latest_at: DateTime<Utc>,
}

fn require_dm(kind: &str) -> ApiResult<()> {
    if kind == "dm" {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "unsupported_channel",
            "solo los mensajes directos tienen estado de lectura",
        ))
    }
}

/// Records that `me` received (or read) the conversation with `peer` up to
/// `up_to`, and tells both sides. Cursors only move forward.
pub async fn mark(
    state: &AppState,
    me: Uuid,
    kind: &str,
    peer: Uuid,
    receipt_state: ReceiptState,
    up_to: DateTime<Utc>,
) -> ApiResult<Receipt> {
    require_dm(kind)?;
    rate_limit(state, &format!("receipt:{me}"), 120, 10).await?;
    let (channel, members) = authorize(state, me, kind, peer).await?;
    // A client can't acknowledge what hasn't been sent yet.
    let up_to = up_to.min(Utc::now());
    let read_at = match receipt_state {
        ReceiptState::Read => up_to,
        ReceiptState::Delivered => DateTime::UNIX_EPOCH,
    };
    let (delivered_at, read_at): (DateTime<Utc>, DateTime<Utc>) = sqlx::query_as(
        "INSERT INTO chat_reads (user_id, channel, delivered_at, read_at) VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, channel) DO UPDATE SET
             delivered_at = GREATEST(chat_reads.delivered_at, EXCLUDED.delivered_at),
             read_at = GREATEST(chat_reads.read_at, EXCLUDED.read_at)
         RETURNING delivered_at, read_at",
    )
    .bind(me)
    .bind(&channel)
    .bind(up_to)
    .bind(read_at)
    .fetch_one(&state.db)
    .await?;
    let receipt = Receipt { delivered_at, read_at };
    // The peer sees the ticks; my other sessions clear their badge.
    state
        .realtime
        .emit(
            &members,
            &Event::ChatReceipt {
                channel,
                user_id: me,
                receipt,
            },
        )
        .await;
    Ok(receipt)
}

/// How far `peer` has received and read what `me` sent them.
pub async fn peer_receipt(state: &AppState, me: Uuid, kind: &str, peer: Uuid) -> ApiResult<Receipt> {
    require_dm(kind)?;
    let (channel, _) = authorize(state, me, kind, peer).await?;
    let found: Option<(DateTime<Utc>, DateTime<Utc>)> =
        sqlx::query_as("SELECT delivered_at, read_at FROM chat_reads WHERE user_id = $1 AND channel = $2")
            .bind(peer)
            .bind(&channel)
            .fetch_optional(&state.db)
            .await?;
    Ok(found.map_or_else(Receipt::none, |(delivered_at, read_at)| Receipt {
        delivered_at,
        read_at,
    }))
}

/// Unread direct messages from each friend.
pub async fn unread(state: &AppState, me: Uuid) -> ApiResult<Vec<Unread>> {
    let friends = friend_ids(state, me).await?;
    if friends.is_empty() {
        return Ok(Vec::new());
    }
    let channels: Vec<String> = friends.iter().map(|f| dm_key(me, *f)).collect();
    let rows: Vec<(String, i64, DateTime<Utc>)> = sqlx::query_as(
        "SELECT m.channel, count(*), max(m.created_at)
         FROM messages m
         LEFT JOIN chat_reads r ON r.user_id = $1 AND r.channel = m.channel
         WHERE m.channel = ANY($2) AND m.sender_id <> $1 AND m.deleted_at IS NULL
           AND m.created_at > COALESCE(r.read_at, 'epoch'::timestamptz)
         GROUP BY m.channel",
    )
    .bind(me)
    .bind(&channels)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(channel, count, latest_at)| {
            let index = channels.iter().position(|c| *c == channel)?;
            Some(Unread {
                peer_id: friends[index],
                count,
                latest_at,
            })
        })
        .collect())
}
