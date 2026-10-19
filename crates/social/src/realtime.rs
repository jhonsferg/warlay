use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::auth::resolve_access_token;
use crate::error::{ApiError, ApiResult};
use crate::models::{friend_ids, Message, Notification, Presence, PresenceStatus, UserSummary, UserWithPresence};
use crate::state::AppState;

const EVENTS_CHANNEL: &str = "wl:events";
/// How long one socket counts as alive without a heartbeat (its node died).
const CONN_FIELD_TTL_SECS: i64 = 90;
/// Events queued for one socket before it counts as stuck and is dropped
/// (the client reconnects and reloads its state). Slots cost nothing until
/// used (events are shared `Arc`s), so it can absorb a burst.
const OUTBOX_CAPACITY: usize = 2048;
/// A socket that sends nothing for this long is dead (clients ping every
/// 10s, older ones every 20s); without it a vanished peer would hold its
/// socket and keep the user "online" until TCP gives up, which can take
/// hours.
const IDLE_TIMEOUT: Duration = Duration::from_secs(75);
/// A write the peer doesn't take within this long means it's gone.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// How long coalesced broadcasts wait for a newer version of themselves.
const COALESCE_WINDOW: Duration = Duration::from_millis(250);
/// Audiences larger than this travel by reference: the list is stored once
/// in Redis (`wl:aud:<hash of its members>`) and envelopes carry the hash.
/// Inline, every message to a 3000-member community put ~120 KB on the bus
/// for every node, and a burst of them outran the subscribers until Redis
/// cut them off (losing everything in flight).
const AUDIENCE_INLINE_MAX: usize = 64;
const AUDIENCE_TTL_SECS: u64 = 300;
/// A stored audience is re-stored (TTL refreshed) after this long in use.
const AUDIENCE_REFRESH: Duration = Duration::from_secs(60);
/// Bound on the audiences each node remembers (stored or resolved).
const AUDIENCE_CACHE_MAX: usize = 4096;

/// Everything the server pushes over the realtime socket.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    Presence {
        user_id: Uuid,
        presence: Presence,
    },
    FriendRequest {
        request: serde_json::Value,
    },
    FriendAdded {
        friend: UserWithPresence,
    },
    FriendRemoved {
        user_id: Uuid,
    },
    Notification {
        notification: Notification,
    },
    Message {
        message: Message,
    },
    /// A message was deleted by its sender.
    MessageDeleted {
        channel: String,
        message_id: Uuid,
    },
    /// `user_id` received/read a DM conversation up to these times.
    ChatReceipt {
        channel: String,
        user_id: Uuid,
        receipt: crate::receipts::Receipt,
    },
    RoomUpdated {
        room: serde_json::Value,
    },
    RoomClosed {
        room_id: Uuid,
    },
    /// The room's helper went away and another one took over: members move
    /// their mesh to it (their established links keep running).
    RoomHelperChanged {
        room_id: Uuid,
        helper: crate::helpers::HelperInfo,
    },
    Invite {
        invite: serde_json::Value,
    },
    CommunityUpdated {
        community_id: Uuid,
    },
    /// Sent to a user's other sessions when they log out everywhere.
    SessionRevoked,
}

#[derive(Serialize)]
struct Envelope {
    targets: Vec<Uuid>,
    /// Deliver to every connected user, ignoring `targets`.
    all: bool,
    /// Events with the same key replace each other within
    /// `COALESCE_WINDOW`: only the latest goes out (to its own targets).
    #[serde(skip_serializing_if = "Option::is_none")]
    coalesce: Option<String>,
    /// Hash of a stored audience standing in for `targets`.
    #[serde(skip_serializing_if = "Option::is_none")]
    audience: Option<String>,
    event: serde_json::Value,
}

/// What every node reads off the bus: the event passes through unparsed.
#[derive(Deserialize)]
struct IncomingEnvelope {
    targets: Vec<Uuid>,
    #[serde(default)]
    all: bool,
    #[serde(default)]
    coalesce: Option<String>,
    #[serde(default)]
    audience: Option<String>,
    event: Box<serde_json::value::RawValue>,
}

fn audience_key(hash: &str) -> String {
    format!("wl:aud:{hash}")
}

/// Identifies a set of users regardless of order.
fn audience_hash(targets: &[Uuid]) -> String {
    use sha2::{Digest, Sha256};
    let mut sorted = targets.to_vec();
    sorted.sort_unstable();
    let mut hasher = Sha256::new();
    for id in &sorted {
        hasher.update(id.as_bytes());
    }
    hasher.finalize()[..16].iter().map(|b| format!("{b:02x}")).collect()
}

/// An event already serialized, shared by every socket it goes to.
type Outgoing = Arc<str>;

/// A coalesced event: its targets (`None`: everyone) and its text.
type Coalesced = (Option<Arc<[Uuid]>>, Outgoing);

/// Each user's open sockets on this instance, by connection id.
type LocalConnections = HashMap<Uuid, Vec<(u64, mpsc::Sender<Outgoing>)>>;

/// Fans events out to connected sockets. Every event goes through Redis
/// pub/sub, so with several main-server instances behind a load balancer
/// each one delivers to the sockets it holds.
pub struct Realtime {
    /// Identifies this process among all nodes (main server and edges).
    pub node_id: Uuid,
    conns: RwLock<LocalConnections>,
    next_id: AtomicU64,
    redis: ConnectionManager,
    /// Events waiting for the next flush, latest per coalescing key, with
    /// their targets (`None`: everyone).
    coalesced: Mutex<HashMap<String, Coalesced>>,
    /// Coalesced events this node will publish at the next flush, latest
    /// per key: a burst (thousands joining a community) must not become a
    /// burst on the bus either, where it can outrun the subscribers until
    /// Redis cuts them off.
    unpublished: Mutex<HashMap<String, Envelope>>,
    /// Audiences this node stored in Redis, and when.
    stored_audiences: Mutex<HashMap<String, std::time::Instant>>,
    /// Audiences this node resolved from Redis, by hash.
    audiences: Mutex<HashMap<String, Arc<[Uuid]>>>,
}

impl Realtime {
    pub fn new(redis: ConnectionManager) -> Arc<Self> {
        Arc::new(Self {
            node_id: Uuid::new_v4(),
            conns: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            redis,
            coalesced: Mutex::new(HashMap::new()),
            unpublished: Mutex::new(HashMap::new()),
            stored_audiences: Mutex::new(HashMap::new()),
            audiences: Mutex::new(HashMap::new()),
        })
    }

    /// Replaces a large target list by a reference to it stored in Redis.
    /// Falls back to sending it inline when it can't be stored.
    async fn externalize_audience(&self, envelope: &mut Envelope) {
        if envelope.all || envelope.targets.len() <= AUDIENCE_INLINE_MAX {
            return;
        }
        let hash = audience_hash(&envelope.targets);
        let fresh = self
            .stored_audiences
            .lock()
            .unwrap()
            .get(&hash)
            .is_some_and(|at| at.elapsed() < AUDIENCE_REFRESH);
        if !fresh {
            let json = serde_json::to_string(&envelope.targets).expect("ids serialize");
            let mut redis = self.redis.clone();
            let stored: Result<(), _> = redis.set_ex(audience_key(&hash), json, AUDIENCE_TTL_SECS).await;
            if stored.is_err() {
                return;
            }
            let mut stored = self.stored_audiences.lock().unwrap();
            if stored.len() >= AUDIENCE_CACHE_MAX {
                stored.retain(|_, at| at.elapsed() < AUDIENCE_REFRESH);
            }
            stored.insert(hash.clone(), std::time::Instant::now());
        }
        envelope.targets = Vec::new();
        envelope.audience = Some(hash);
    }

    /// The members of a stored audience, from this node's cache or Redis.
    async fn resolve_audience(&self, hash: &str) -> Option<Arc<[Uuid]>> {
        if let Some(found) = self.audiences.lock().unwrap().get(hash) {
            return Some(found.clone());
        }
        let mut redis = self.redis.clone();
        let json: Option<String> = redis.get(audience_key(hash)).await.ok()?;
        let ids: Arc<[Uuid]> = serde_json::from_str::<Vec<Uuid>>(&json?).ok()?.into();
        let mut cache = self.audiences.lock().unwrap();
        if cache.len() >= AUDIENCE_CACHE_MAX {
            cache.clear();
        }
        cache.insert(hash.to_string(), ids.clone());
        Some(ids)
    }

    /// Whether an event under `key` is already waiting to be published, so
    /// a content-free refresh needn't be built again.
    pub fn coalescing(&self, key: &str) -> bool {
        self.unpublished.lock().unwrap().contains_key(key)
    }

    /// Publishes and delivers the coalesced events every `COALESCE_WINDOW`.
    fn start_coalescer(self: &Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(COALESCE_WINDOW);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let outgoing = std::mem::take(&mut *this.unpublished.lock().unwrap());
                if !outgoing.is_empty() {
                    let mut pipe = redis::pipe();
                    for mut envelope in outgoing.into_values() {
                        this.externalize_audience(&mut envelope).await;
                        pipe.publish(
                            EVENTS_CHANNEL,
                            serde_json::to_string(&envelope).expect("envelopes serialize"),
                        )
                        .ignore();
                    }
                    let mut redis = this.redis.clone();
                    if let Err(error) = pipe.query_async::<()>(&mut redis).await {
                        warn!(%error, "failed to publish realtime events");
                    }
                }
                let batch = std::mem::take(&mut *this.coalesced.lock().unwrap());
                for (targets, text) in batch.values() {
                    match targets {
                        Some(targets) => this.deliver_local(targets, text),
                        None => this.deliver_all_local(text),
                    }
                }
            }
        });
    }

    /// Subscribes to the event channel; reconnects if Redis drops.
    pub fn start_subscriber(self: &Arc<Self>, client: redis::Client) {
        self.start_coalescer();
        let this = self.clone();
        tokio::spawn(async move {
            // Events published while unsubscribed are lost: come back fast
            // (a Redis restart takes a second or two), then back off.
            let mut backoff = Duration::from_millis(100);
            loop {
                match client.get_async_pubsub().await {
                    Ok(mut pubsub) => {
                        if let Err(error) = pubsub.subscribe(EVENTS_CHANNEL).await {
                            warn!(%error, "failed to subscribe to realtime events");
                        } else {
                            backoff = Duration::from_millis(100);
                            let mut stream = pubsub.on_message();
                            while let Some(msg) = stream.next().await {
                                let Ok(payload) = msg.get_payload::<String>() else {
                                    continue;
                                };
                                match serde_json::from_str::<IncomingEnvelope>(&payload) {
                                    Ok(envelope) => {
                                        let text: Outgoing = envelope.event.get().into();
                                        let targets: Arc<[Uuid]> = match &envelope.audience {
                                            Some(hash) => match this.resolve_audience(hash).await {
                                                Some(targets) => targets,
                                                None => {
                                                    warn!(%hash, "realtime event for an unknown audience, dropped");
                                                    continue;
                                                }
                                            },
                                            None => envelope.targets.into(),
                                        };
                                        if let Some(key) = envelope.coalesce {
                                            let targets = (!envelope.all).then_some(targets);
                                            this.coalesced.lock().unwrap().insert(key, (targets, text));
                                        } else if envelope.all {
                                            this.deliver_all_local(&text);
                                        } else {
                                            this.deliver_local(&targets, &text);
                                        }
                                    }
                                    Err(error) => debug!(%error, "malformed realtime envelope"),
                                }
                            }
                            warn!("realtime subscriber lost Redis, resubscribing");
                        }
                    }
                    Err(error) => warn!(%error, "realtime subscriber cannot reach Redis"),
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(2));
            }
        });
    }

    pub async fn emit(&self, targets: &[Uuid], event: &Event) {
        self.emit_coalesced(targets, event, None).await;
    }

    /// `emit`, but events under the same `coalesce` key replace each other
    /// within `COALESCE_WINDOW` and only the latest goes out: for state
    /// refreshes sent to large audiences (a community's members) that a
    /// burst of changes would otherwise multiply.
    pub async fn emit_coalesced(&self, targets: &[Uuid], event: &Event, coalesce: Option<String>) {
        if targets.is_empty() {
            return;
        }
        self.publish(Envelope {
            targets: targets.to_vec(),
            all: false,
            coalesce,
            audience: None,
            event: serde_json::to_value(event).expect("events serialize"),
        })
        .await;
    }

    /// Sends `event` to every connected user (public lobby updates). With a
    /// `coalesce` key, a burst of broadcasts under that key (a room filling
    /// up) reaches each socket once per `COALESCE_WINDOW`, latest state only:
    /// these broadcasts cost one send per online user each.
    pub async fn emit_all(&self, event: &Event, coalesce: Option<String>) {
        self.publish(Envelope {
            targets: Vec::new(),
            all: true,
            coalesce,
            audience: None,
            event: serde_json::to_value(event).expect("events serialize"),
        })
        .await;
    }

    async fn publish(&self, mut envelope: Envelope) {
        if let Some(key) = &envelope.coalesce {
            self.unpublished.lock().unwrap().insert(key.clone(), envelope);
            return;
        }
        self.externalize_audience(&mut envelope).await;
        let mut redis = self.redis.clone();
        let payload = serde_json::to_string(&envelope).expect("envelopes serialize");
        if let Err(error) = redis.publish::<_, _, ()>(EVENTS_CHANNEL, payload).await {
            warn!(%error, "failed to publish realtime event");
        }
    }

    fn deliver_local(&self, targets: &[Uuid], text: &Outgoing) {
        let mut stuck = Vec::new();
        {
            let conns = self.conns.read().unwrap();
            for target in targets {
                for (id, tx) in conns.get(target).into_iter().flatten() {
                    if let Err(mpsc::error::TrySendError::Full(_)) = tx.try_send(text.clone()) {
                        stuck.push((*target, *id));
                    }
                }
            }
        }
        self.drop_stuck(stuck);
    }

    fn deliver_all_local(&self, text: &Outgoing) {
        let mut stuck = Vec::new();
        {
            let conns = self.conns.read().unwrap();
            for (user_id, list) in conns.iter() {
                for (id, tx) in list {
                    if let Err(mpsc::error::TrySendError::Full(_)) = tx.try_send(text.clone()) {
                        stuck.push((*user_id, *id));
                    }
                }
            }
        }
        self.drop_stuck(stuck);
    }

    /// Sockets that stopped draining their queue: forgetting their sender
    /// ends them, instead of buffering events for them without limit.
    fn drop_stuck(&self, stuck: Vec<(Uuid, u64)>) {
        for (user_id, conn_id) in stuck {
            warn!(%user_id, conn_id, "dropping a realtime socket that stopped reading");
            self.unregister(user_id, conn_id);
        }
    }

    /// Registers a socket. The registry keeps the only strong sender (the
    /// socket gets a weak one for its own replies), so forgetting a stuck
    /// socket here closes its receiver and ends it.
    fn register(&self, user_id: Uuid) -> (u64, mpsc::WeakSender<Outgoing>, mpsc::Receiver<Outgoing>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel(OUTBOX_CAPACITY);
        let weak = tx.downgrade();
        self.conns.write().unwrap().entry(user_id).or_default().push((id, tx));
        (id, weak, rx)
    }

    /// Sockets open on this node, for load reporting.
    pub fn local_connections(&self) -> usize {
        self.conns.read().unwrap().values().map(Vec::len).sum()
    }

    fn unregister(&self, user_id: Uuid, conn_id: u64) {
        let mut conns = self.conns.write().unwrap();
        if let Some(list) = conns.get_mut(&user_id) {
            list.retain(|(id, _)| *id != conn_id);
            if list.is_empty() {
                conns.remove(&user_id);
            }
        }
    }
}

pub(crate) fn conns_key(user_id: Uuid) -> String {
    format!("wl:conns:{user_id}")
}

/// Live presence for each of `user_ids`: online-ness from Redis (any open
/// socket on any instance), room/in-game from the database.
pub async fn presence_of(state: &AppState, user_ids: &[Uuid]) -> ApiResult<HashMap<Uuid, Presence>> {
    let mut result = HashMap::new();
    if user_ids.is_empty() {
        return Ok(result);
    }
    let mut redis = state.redis.clone();
    let mut pipe = redis::pipe();
    for id in user_ids {
        pipe.hlen(conns_key(*id));
    }
    let counts: Vec<i64> = pipe.query_async(&mut redis).await?;

    let rooms: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT m.user_id, r.id, r.status FROM room_members m JOIN rooms r ON r.id = m.room_id
         WHERE m.user_id = ANY($1) AND r.status <> 'closed'",
    )
    .bind(user_ids)
    .fetch_all(&state.db)
    .await?;
    let rooms: HashMap<Uuid, (Uuid, String)> = rooms.into_iter().map(|(u, r, s)| (u, (r, s))).collect();

    for (id, count) in user_ids.iter().zip(counts) {
        let online = count > 0;
        let presence = match (online, rooms.get(id)) {
            (false, _) => Presence {
                status: PresenceStatus::Offline,
                room_id: None,
            },
            (true, Some((room_id, status))) => Presence {
                status: if status == "in_game" {
                    PresenceStatus::InGame
                } else {
                    PresenceStatus::InRoom
                },
                room_id: Some(*room_id),
            },
            (true, None) => Presence {
                status: PresenceStatus::Online,
                room_id: None,
            },
        };
        result.insert(*id, presence);
    }
    Ok(result)
}

pub async fn with_presence(state: &AppState, users: Vec<UserSummary>) -> ApiResult<Vec<UserWithPresence>> {
    let ids: Vec<Uuid> = users.iter().map(|u| u.id).collect();
    let presence = presence_of(state, &ids).await?;
    Ok(users
        .into_iter()
        .map(|user| {
            let presence = presence.get(&user.id).cloned().unwrap_or(Presence {
                status: PresenceStatus::Offline,
                room_id: None,
            });
            UserWithPresence { user, presence }
        })
        .collect())
}

/// Tells `user_id`'s friends (and the user's own other sessions) about
/// their current presence.
pub async fn broadcast_presence(state: &AppState, user_id: Uuid) -> ApiResult<()> {
    let presence = presence_of(state, &[user_id])
        .await?
        .remove(&user_id)
        .expect("presence for the requested user");
    let mut targets = friend_ids(state, user_id).await?;
    targets.push(user_id);
    state
        .realtime
        .emit(&targets, &Event::Presence { user_id, presence })
        .await;
    Ok(())
}

#[derive(Deserialize)]
pub struct WsQuery {
    token: String,
}

/// `GET /api/v1/ws?token=<access token>` -- the realtime channel.
pub async fn ws_handler(
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let (user_id, _) = resolve_access_token(&state, &query.token)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    Ok(upgrade.on_upgrade(move |socket| run_socket(state, user_id, socket)))
}

async fn run_socket(state: AppState, user_id: Uuid, socket: WebSocket) {
    let (conn_id, self_tx, mut rx) = state.realtime.register(user_id);
    let mut redis = state.redis.clone();
    // One hash field per open socket, each with its own expiry (Redis 7.4+
    // hash-field TTLs): a node that dies takes its sockets' fields with it
    // within a minute and a half, instead of leaving users "online" forever.
    let key = conns_key(user_id);
    let field = format!("{}:{conn_id}", state.realtime.node_id);
    let count: i64 = redis::pipe()
        .hset(&key, &field, 1)
        .ignore()
        .cmd("HEXPIRE")
        .arg(&key)
        .arg(CONN_FIELD_TTL_SECS)
        .arg("FIELDS")
        .arg(1)
        .arg(&field)
        .ignore()
        .hlen(&key)
        .query_async::<(i64,)>(&mut redis)
        .await
        .map(|(n,)| n)
        .unwrap_or(1);
    if count == 1 {
        info!(%user_id, "user online");
        let _ = broadcast_presence(&state, user_id).await;
        let _ = crate::routes::rooms::host_came_back(&state, user_id).await;
    }

    let (mut sink, mut stream) = socket.split();
    let _ = sink
        .send(WsMessage::Text(r#"{"type":"hello"}"#.to_string().into()))
        .await;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    let idle = tokio::time::sleep(IDLE_TIMEOUT);
    tokio::pin!(idle);
    loop {
        tokio::select! {
            outgoing = rx.recv() => match outgoing {
                Some(text) => {
                    let sent = tokio::time::timeout(SEND_TIMEOUT, sink.send(WsMessage::Text(text.as_ref().into()))).await;
                    if !matches!(sent, Ok(Ok(()))) {
                        break;
                    }
                }
                None => break,
            },
            () = &mut idle => {
                debug!(%user_id, "realtime socket idle, closing it");
                break;
            }
            incoming = stream.next() => {
                idle.as_mut().reset(tokio::time::Instant::now() + IDLE_TIMEOUT);
                match incoming {
                    Some(Ok(WsMessage::Text(text))) if text.as_str() == "ping" => {
                        let sent = tokio::time::timeout(SEND_TIMEOUT, sink.send(WsMessage::Text("pong".into()))).await;
                        if !matches!(sent, Ok(Ok(()))) {
                            break;
                        }
                    }
                    Some(Ok(WsMessage::Text(text))) => {
                        // Client requests ride the same socket (chat works through a
                        // helper even with the main server down). Answered async so
                        // a slow request never stalls event delivery.
                        if let Ok(request) = serde_json::from_str::<ClientRequest>(text.as_str()) {
                            let state = state.clone();
                            let tx = self_tx.clone();
                            tokio::spawn(async move {
                                let reply: Outgoing = handle_request(&state, user_id, request).await.to_string().into();
                                if let Some(tx) = tx.upgrade() {
                                    let _ = tx.send(reply).await;
                                }
                            });
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => {}
                }
            },
            _ = heartbeat.tick() => {
                let _: Result<(), _> = redis::cmd("HEXPIRE")
                    .arg(&key)
                    .arg(CONN_FIELD_TTL_SECS)
                    .arg("FIELDS")
                    .arg(1)
                    .arg(&field)
                    .query_async(&mut redis)
                    .await;
            }
        }
    }

    state.realtime.unregister(user_id, conn_id);
    let remaining: i64 = redis::pipe()
        .hdel(&key, &field)
        .ignore()
        .hlen(&key)
        .query_async::<(i64,)>(&mut redis)
        .await
        .map(|(n,)| n)
        .unwrap_or(0);
    if remaining <= 0 {
        info!(%user_id, "user offline");
        let _ = sqlx::query("UPDATE users SET last_seen_at = now() WHERE id = $1")
            .bind(user_id)
            .execute(&state.db)
            .await;
        let _ = broadcast_presence(&state, user_id).await;
        // A player who doesn't come back within the grace period leaves
        // their room (closing it if they host it).
        let grace = Duration::from_secs(state.settings.disconnect_grace_secs);
        tokio::spawn(async move {
            tokio::time::sleep(grace).await;
            let mut redis = state.redis.clone();
            let count: i64 = redis.hlen(conns_key(user_id)).await.unwrap_or(0);
            if count <= 0 {
                if let Err(error) = crate::routes::rooms::disconnect_grace_expired(&state, user_id).await {
                    warn!(?error, %user_id, "failed to handle a disconnected player's room");
                }
            }
        });
    }
}

/// Requests a client may send over the realtime socket.
#[derive(Deserialize)]
#[serde(tag = "type")]
enum ClientRequest {
    #[serde(rename = "chat.send", rename_all = "camelCase")]
    ChatSend {
        request_id: String,
        kind: String,
        target_id: Uuid,
        body: String,
    },
}

async fn handle_request(state: &AppState, user_id: Uuid, request: ClientRequest) -> serde_json::Value {
    match request {
        ClientRequest::ChatSend {
            request_id,
            kind,
            target_id,
            body,
        } => match crate::chat::send(state, user_id, &kind, target_id, &body, Some(&request_id)).await {
            Ok(message) => serde_json::json!({ "type": "chat.ack", "requestId": request_id, "message": message }),
            Err(error) => serde_json::json!({
                "type": "chat.error", "requestId": request_id, "error": error.code, "message": error.message
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audience_hash_names_the_set_not_the_order() {
        let ids: Vec<Uuid> = (0..100).map(|_| Uuid::new_v4()).collect();
        let mut shuffled = ids.clone();
        shuffled.reverse();
        assert_eq!(audience_hash(&ids), audience_hash(&shuffled));
        assert_eq!(audience_hash(&ids).len(), 32);
        assert_ne!(audience_hash(&ids), audience_hash(&ids[1..]));
    }
}
