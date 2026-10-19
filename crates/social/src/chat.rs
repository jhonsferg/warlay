//! Chat as a service shared by the main server and every helper: sending
//! and reading messages never needs the main server, so chat survives it
//! going down. The hot path only touches Redis:
//!
//! - who may read/write a channel is cached (`wl:chan:<key>`),
//! - a new message goes to a Redis stream (`wl:chat:stream`), to the
//!   channel's recent-history list (`wl:chat:recent:<key>`) and out on the
//!   realtime bus, all in one round trip,
//! - a persister (running on every node, as one consumer group) drains the
//!   stream into PostgreSQL in batches, so a node dying mid-batch just
//!   leaves its messages to be claimed by another.

use std::time::Duration;

use chrono::{DateTime, Utc};
use redis::streams::{StreamAutoClaimOptions, StreamAutoClaimReply, StreamReadOptions, StreamReadReply};
use redis::AsyncCommands;
use sqlx::FromRow;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::auth::rate_limit;
use crate::error::{ApiError, ApiResult};
use crate::models::{are_friends, load_summaries, Message, UserSummary};
use crate::realtime::Event;
use crate::routes::communities::{member_ids, role_in};
use crate::routes::rooms::room_member_ids;
use crate::state::AppState;

pub const MAX_BODY_CHARS: usize = 2000;
const STREAM: &str = "wl:chat:stream";
const GROUP: &str = "persist";
const RECENT_LEN: isize = 200;
const CHANNEL_TTL_SECS: u64 = 300;
const USER_TTL_SECS: u64 = 600;
/// How long a send's request id is remembered for deduplication.
const REQUEST_TTL_SECS: u64 = 600;

pub fn dm_key(a: Uuid, b: Uuid) -> String {
    format!("dm:{}:{}", a.min(b), a.max(b))
}

pub fn channel_key(kind: &str, me: Uuid, id: Uuid) -> ApiResult<String> {
    match kind {
        "dm" => Ok(dm_key(me, id)),
        "room" => Ok(format!("room:{id}")),
        "community" => Ok(format!("community:{id}")),
        _ => Err(ApiError::not_found("canal desconocido")),
    }
}

/// Drops the cached audience of a channel after a membership change.
pub async fn invalidate(state: &AppState, key: &str) {
    let mut redis = state.redis.clone();
    let _: Result<(), _> = redis.del(format!("wl:chan:{key}")).await;
}

pub async fn invalidate_user(state: &AppState, user_id: Uuid) {
    let mut redis = state.redis.clone();
    let _: Result<(), _> = redis.del(format!("wl:user:{user_id}")).await;
}

/// Everyone who may read `key`, from cache or the database.
async fn audience(state: &AppState, kind: &str, me: Uuid, id: Uuid, key: &str) -> ApiResult<Vec<Uuid>> {
    let cache_key = format!("wl:chan:{key}");
    let mut redis = state.redis.clone();
    if let Some(cached) = redis.get::<_, Option<String>>(&cache_key).await? {
        if let Ok(ids) = serde_json::from_str::<Vec<Uuid>>(&cached) {
            return Ok(ids);
        }
    }
    let ids = match kind {
        "dm" => {
            if are_friends(state, me, id).await? {
                vec![me, id]
            } else {
                Vec::new()
            }
        }
        "room" => room_member_ids(state, id).await?,
        _ => member_ids(state, id).await?,
    };
    let json = serde_json::to_string(&ids).map_err(ApiError::internal)?;
    let _: () = redis.set_ex(&cache_key, json, CHANNEL_TTL_SECS).await?;
    Ok(ids)
}

/// The channel key plus its audience, if `me` may use it.
pub(crate) async fn authorize(state: &AppState, me: Uuid, kind: &str, id: Uuid) -> ApiResult<(String, Vec<Uuid>)> {
    let key = channel_key(kind, me, id)?;
    let members = audience(state, kind, me, id, &key).await?;
    if members.contains(&me) {
        return Ok((key, members));
    }
    // A stale cache could be the reason; check once more at the source.
    invalidate(state, &key).await;
    let members = audience(state, kind, me, id, &key).await?;
    if members.contains(&me) {
        return Ok((key, members));
    }
    Err(ApiError::forbidden(match kind {
        "dm" => "solo puedes chatear en privado con tus amigos",
        "room" => "no estás en esa sala",
        _ => {
            // Keeps the old precise check for communities (role table).
            if role_in(state, id, me).await?.is_none() {
                "no eres miembro de esa comunidad"
            } else {
                "no puedes escribir aquí"
            }
        }
    }))
}

/// A user's public summary, cached.
pub async fn user_summary(state: &AppState, user_id: Uuid) -> ApiResult<UserSummary> {
    let key = format!("wl:user:{user_id}");
    let mut redis = state.redis.clone();
    if let Some(cached) = redis.get::<_, Option<String>>(&key).await? {
        if let Ok(user) = serde_json::from_str::<UserSummary>(&cached) {
            return Ok(user);
        }
    }
    let user = load_summaries(state, &[user_id])
        .await?
        .pop()
        .ok_or_else(ApiError::unauthorized)?;
    let json = serde_json::to_string(&user).map_err(ApiError::internal)?;
    let _: () = redis.set_ex(&key, json, USER_TTL_SECS).await?;
    Ok(user)
}

/// Posts a message. `request_id`, when given, makes it idempotent: the same
/// id from the same user returns the message already posted (a send that
/// timed out on the socket and was retried over HTTP, or reached a frozen
/// edge that processed it later) instead of posting it twice.
pub async fn send(
    state: &AppState,
    me: Uuid,
    kind: &str,
    id: Uuid,
    body: &str,
    request_id: Option<&str>,
) -> ApiResult<Message> {
    let dedup_key = request_id
        .filter(|r| !r.is_empty() && r.len() <= 64)
        .map(|r| format!("wl:chatreq:{me}:{r}"));
    let mut redis = state.redis.clone();
    if let Some(key) = &dedup_key {
        if let Some(sent) = redis.get::<_, Option<String>>(key).await? {
            if let Ok(message) = serde_json::from_str::<Message>(&sent) {
                return Ok(message);
            }
        }
    }
    let body = body.trim();
    if body.is_empty() {
        return Err(ApiError::bad_request("empty_message", "el mensaje está vacío"));
    }
    if body.chars().count() > MAX_BODY_CHARS {
        return Err(ApiError::bad_request(
            "message_too_long",
            "el mensaje es demasiado largo",
        ));
    }
    rate_limit(state, &format!("chat:{me}"), 30, 10).await?;
    let (key, members) = authorize(state, me, kind, id).await?;
    let message = Message {
        id: Uuid::now_v7(),
        channel: key.clone(),
        sender: user_summary(state, me).await?,
        body: body.to_string(),
        // Microseconds, like PostgreSQL stores them: read cursors are compared
        // with this time on both sides, and a nanosecond tail would leave
        // every message just past the cursor that acknowledged it.
        created_at: DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap_or_else(Utc::now),
    };
    let json = serde_json::to_string(&message).map_err(ApiError::internal)?;
    if let Some(dedup_key) = &dedup_key {
        // Claims the request id; losing the race means a concurrent attempt
        // of this same send got there first.
        let claimed: Option<String> = redis::cmd("SET")
            .arg(dedup_key)
            .arg(&json)
            .arg("NX")
            .arg("EX")
            .arg(REQUEST_TTL_SECS)
            .query_async(&mut redis)
            .await?;
        if claimed.is_none() {
            let sent: Option<String> = redis.get(dedup_key).await?;
            if let Some(message) = sent.and_then(|s| serde_json::from_str::<Message>(&s).ok()) {
                return Ok(message);
            }
        }
    }
    let recent = format!("wl:chat:recent:{key}");
    let _: () = redis::pipe()
        .cmd("XADD")
        .arg(STREAM)
        .arg("MAXLEN")
        .arg("~")
        .arg(500_000)
        .arg("*")
        .arg("m")
        .arg(&json)
        .ignore()
        .lpush(&recent, &json)
        .ignore()
        .ltrim(&recent, 0, RECENT_LEN - 1)
        .ignore()
        .expire(&recent, 7 * 24 * 3600)
        .ignore()
        .query_async(&mut redis)
        .await?;
    state
        .realtime
        .emit(
            &members,
            &Event::Message {
                message: message.clone(),
            },
        )
        .await;
    Ok(message)
}

#[derive(FromRow)]
struct MessageRow {
    id: Uuid,
    channel: String,
    sender_id: Uuid,
    body: String,
    created_at: DateTime<Utc>,
}

async fn from_db(state: &AppState, key: &str, before: Option<DateTime<Utc>>, limit: i64) -> ApiResult<Vec<Message>> {
    let rows = sqlx::query_as::<_, MessageRow>(
        "SELECT id, channel, sender_id, body, created_at FROM messages
         WHERE channel = $1 AND deleted_at IS NULL AND ($2::timestamptz IS NULL OR created_at < $2)
         ORDER BY created_at DESC LIMIT $3",
    )
    .bind(key)
    .bind(before)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    let mut ids: Vec<Uuid> = rows.iter().map(|r| r.sender_id).collect();
    ids.sort();
    ids.dedup();
    let users = load_summaries(state, &ids).await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            Some(Message {
                sender: users.iter().find(|u| u.id == r.sender_id)?.clone(),
                id: r.id,
                channel: r.channel,
                body: r.body,
                created_at: r.created_at,
            })
        })
        .collect())
}

fn tombstone_key(message_id: Uuid) -> String {
    format!("wl:chat:deleted:{message_id}")
}

/// Deletes one of `me`'s own messages. The message may be anywhere on its
/// way: still in the channel's recent list and the persist stream, already
/// in PostgreSQL, or both. A tombstone in Redis covers the first case: the
/// persister marks the row as deleted when it finally writes it.
pub async fn delete_message(state: &AppState, me: Uuid, kind: &str, id: Uuid, message_id: Uuid) -> ApiResult<()> {
    let (key, members) = authorize(state, me, kind, id).await?;
    let mut redis = state.redis.clone();
    let recent_key = format!("wl:chat:recent:{key}");
    let cached: Vec<String> = redis.lrange(&recent_key, 0, -1).await?;
    let in_recent = cached
        .into_iter()
        .find_map(|json| serde_json::from_str::<Message>(&json).ok().filter(|m| m.id == message_id).map(|m| (json, m)));
    let in_db: Option<(Uuid, Option<DateTime<Utc>>)> =
        sqlx::query_as("SELECT sender_id, deleted_at FROM messages WHERE id = $1 AND channel = $2")
            .bind(message_id)
            .bind(&key)
            .fetch_optional(&state.db)
            .await?;
    let sender = match (&in_recent, &in_db) {
        (Some((_, message)), _) => message.sender.id,
        (None, Some((sender, _))) => *sender,
        (None, None) => return Err(ApiError::not_found("el mensaje no existe")),
    };
    if sender != me {
        return Err(ApiError::forbidden("solo puedes borrar tus propios mensajes"));
    }
    if in_db.as_ref().is_some_and(|(_, deleted_at)| deleted_at.is_some()) {
        return Ok(());
    }
    let _: () = redis.set_ex(tombstone_key(message_id), "1", 24 * 3600).await?;
    if let Some((json, _)) = in_recent {
        let _: i64 = redis.lrem(&recent_key, 1, json).await?;
    }
    sqlx::query("UPDATE messages SET deleted_at = now() WHERE id = $1 AND channel = $2 AND deleted_at IS NULL")
        .bind(message_id)
        .bind(&key)
        .execute(&state.db)
        .await?;
    state
        .realtime
        .emit(
            &members,
            &Event::MessageDeleted {
                channel: key,
                message_id,
            },
        )
        .await;
    Ok(())
}

/// Newest messages last. The first page comes from the Redis recent list
/// (topped up from the database if the cache is short); older pages come
/// from the database.
pub async fn history(
    state: &AppState,
    me: Uuid,
    kind: &str,
    id: Uuid,
    before: Option<DateTime<Utc>>,
    limit: i64,
) -> ApiResult<Vec<Message>> {
    let (key, _) = authorize(state, me, kind, id).await?;
    let limit = limit.clamp(1, 100);
    let mut newest_first: Vec<Message> = Vec::new();
    if before.is_none() {
        let mut redis = state.redis.clone();
        let cached: Vec<String> = redis
            .lrange(format!("wl:chat:recent:{key}"), 0, limit as isize - 1)
            .await?;
        newest_first = cached
            .iter()
            .filter_map(|j| serde_json::from_str::<Message>(j).ok())
            .collect();
    }
    if (newest_first.len() as i64) < limit {
        let older_than = newest_first.last().map(|m| m.created_at).or(before);
        let missing = limit - newest_first.len() as i64;
        for m in from_db(state, &key, older_than, missing).await? {
            if !newest_first.iter().any(|c| c.id == m.id) {
                newest_first.push(m);
            }
        }
    }
    newest_first.reverse();
    Ok(newest_first)
}

async fn ensure_group(state: &AppState) {
    let mut redis = state.redis.clone();
    let created: Result<(), redis::RedisError> = redis.xgroup_create_mkstream(STREAM, GROUP, "0").await;
    if let Err(error) = created {
        if !error.to_string().contains("BUSYGROUP") {
            warn!(%error, "could not create the chat persister group");
        }
    }
}

async fn persist_batch(state: &AppState, entries: Vec<(String, String)>) -> usize {
    let messages: Vec<Message> = entries
        .iter()
        .filter_map(|(_, json)| serde_json::from_str(json).ok())
        .collect();
    if !messages.is_empty() {
        let ids: Vec<Uuid> = messages.iter().map(|m| m.id).collect();
        let channels: Vec<String> = messages.iter().map(|m| m.channel.clone()).collect();
        let senders: Vec<Uuid> = messages.iter().map(|m| m.sender.id).collect();
        let bodies: Vec<String> = messages.iter().map(|m| m.body.clone()).collect();
        let times: Vec<DateTime<Utc>> = messages.iter().map(|m| m.created_at).collect();
        let inserted = sqlx::query(
            "INSERT INTO messages (id, channel, sender_id, body, created_at)
             SELECT * FROM UNNEST($1::uuid[], $2::text[], $3::uuid[], $4::text[], $5::timestamptz[])
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(&ids)
        .bind(&channels)
        .bind(&senders)
        .bind(&bodies)
        .bind(&times)
        .execute(&state.db)
        .await;
        if let Err(error) = inserted {
            // Leave them unacknowledged: they'll be retried or claimed.
            warn!(%error, count = messages.len(), "failed to persist chat batch");
            return 0;
        }
        // Deleted while still queued: mark the rows now that they exist.
        let mut redis = state.redis.clone();
        let tombstones: Vec<String> = ids.iter().map(|id| tombstone_key(*id)).collect();
        let flags: Result<Vec<Option<String>>, _> = redis.mget(&tombstones).await;
        if let Ok(flags) = flags {
            let deleted: Vec<Uuid> = ids
                .iter()
                .zip(flags)
                .filter_map(|(id, flag)| flag.map(|_| *id))
                .collect();
            if !deleted.is_empty() {
                let _ = sqlx::query("UPDATE messages SET deleted_at = now() WHERE id = ANY($1) AND deleted_at IS NULL")
                    .bind(&deleted)
                    .execute(&state.db)
                    .await;
            }
        }
    }
    let stream_ids: Vec<String> = entries.into_iter().map(|(id, _)| id).collect();
    let mut redis = state.redis.clone();
    let _: Result<i64, _> = redis.xack(STREAM, GROUP, &stream_ids).await;
    stream_ids.len()
}

fn entries_of(ids: Vec<redis::streams::StreamId>) -> Vec<(String, String)> {
    ids.into_iter()
        .filter_map(|e| {
            let json = e.get::<String>("m")?;
            Some((e.id, json))
        })
        .collect()
}

/// Drains the chat stream into PostgreSQL. Run it on every node: they share
/// one consumer group, and messages a dead node had taken get re-claimed.
pub fn spawn_persister(state: AppState, consumer: String) {
    tokio::spawn(async move {
        ensure_group(&state).await;
        info!(%consumer, "chat persister started");
        let mut last_claim = std::time::Instant::now();
        loop {
            let mut redis = state.redis.clone();
            // No BLOCK: this connection is multiplexed with every other
            // command of the node, and a blocking read would stall them all.
            let opts = StreamReadOptions::default().group(GROUP, &consumer).count(500);
            let reply: Result<StreamReadReply, _> = redis.xread_options(&[STREAM], &[">"], &opts).await;
            match reply {
                Ok(reply) => {
                    if reply.keys.iter().all(|k| k.ids.is_empty()) {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    for key in reply.keys {
                        let n = persist_batch(&state, entries_of(key.ids)).await;
                        if n > 0 {
                            debug!(n, "persisted chat messages");
                        }
                    }
                }
                Err(error) => {
                    if error.to_string().contains("NOGROUP") {
                        ensure_group(&state).await;
                    } else {
                        warn!(%error, "chat persister cannot read the stream");
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
            // Messages another node read but never acknowledged (it died).
            if last_claim.elapsed() > Duration::from_secs(15) {
                last_claim = std::time::Instant::now();
                let opts = StreamAutoClaimOptions::default().count(500);
                let claimed: Result<StreamAutoClaimReply, _> = redis
                    .xautoclaim_options(STREAM, GROUP, &consumer, 30_000, "0-0", opts)
                    .await;
                if let Ok(claimed) = claimed {
                    let n = persist_batch(&state, entries_of(claimed.claimed)).await;
                    if n > 0 {
                        info!(n, "persisted chat messages left behind by another node");
                    }
                }
            }
        }
    });
}
