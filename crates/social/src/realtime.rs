use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
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

#[derive(Serialize, Deserialize)]
struct Envelope {
    targets: Vec<Uuid>,
    /// Deliver to every connected user, ignoring `targets`.
    #[serde(default)]
    all: bool,
    event: serde_json::Value,
}

/// Fans events out to connected sockets. Every event goes through Redis
/// pub/sub, so with several main-server instances behind a load balancer
/// each one delivers to the sockets it holds.
/// Each user's open sockets on this instance, by connection id.
type LocalConnections = HashMap<Uuid, Vec<(u64, mpsc::UnboundedSender<String>)>>;

pub struct Realtime {
    /// Identifies this process among all nodes (main server and edges).
    pub node_id: Uuid,
    conns: RwLock<LocalConnections>,
    next_id: AtomicU64,
    redis: ConnectionManager,
}

impl Realtime {
    pub fn new(redis: ConnectionManager) -> Arc<Self> {
        Arc::new(Self {
            node_id: Uuid::new_v4(),
            conns: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            redis,
        })
    }

    /// Subscribes to the event channel; reconnects if Redis drops.
    pub fn start_subscriber(self: &Arc<Self>, client: redis::Client) {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                match client.get_async_pubsub().await {
                    Ok(mut pubsub) => {
                        if let Err(error) = pubsub.subscribe(EVENTS_CHANNEL).await {
                            warn!(%error, "failed to subscribe to realtime events");
                        } else {
                            let mut stream = pubsub.on_message();
                            while let Some(msg) = stream.next().await {
                                let Ok(payload) = msg.get_payload::<String>() else {
                                    continue;
                                };
                                match serde_json::from_str::<Envelope>(&payload) {
                                    Ok(envelope) => {
                                        let text = envelope.event.to_string();
                                        if envelope.all {
                                            this.deliver_all_local(&text);
                                        } else {
                                            this.deliver_local(&envelope.targets, &text);
                                        }
                                    }
                                    Err(error) => debug!(%error, "malformed realtime envelope"),
                                }
                            }
                        }
                    }
                    Err(error) => warn!(%error, "realtime subscriber cannot reach Redis"),
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    pub async fn emit(&self, targets: &[Uuid], event: &Event) {
        if targets.is_empty() {
            return;
        }
        self.publish(Envelope {
            targets: targets.to_vec(),
            all: false,
            event: serde_json::to_value(event).expect("events serialize"),
        })
        .await;
    }

    /// Sends `event` to every connected user (public lobby updates).
    pub async fn emit_all(&self, event: &Event) {
        self.publish(Envelope {
            targets: Vec::new(),
            all: true,
            event: serde_json::to_value(event).expect("events serialize"),
        })
        .await;
    }

    async fn publish(&self, envelope: Envelope) {
        let mut redis = self.redis.clone();
        let payload = serde_json::to_string(&envelope).expect("envelopes serialize");
        if let Err(error) = redis.publish::<_, _, ()>(EVENTS_CHANNEL, payload).await {
            warn!(%error, "failed to publish realtime event");
        }
    }

    fn deliver_local(&self, targets: &[Uuid], text: &str) {
        let conns = self.conns.read().unwrap();
        for target in targets {
            if let Some(list) = conns.get(target) {
                for (_, tx) in list {
                    let _ = tx.send(text.to_string());
                }
            }
        }
    }

    fn deliver_all_local(&self, text: &str) {
        let conns = self.conns.read().unwrap();
        for (_, tx) in conns.values().flatten() {
            let _ = tx.send(text.to_string());
        }
    }

    fn register(&self, user_id: Uuid) -> (u64, mpsc::UnboundedSender<String>, mpsc::UnboundedReceiver<String>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::unbounded_channel();
        self.conns
            .write()
            .unwrap()
            .entry(user_id)
            .or_default()
            .push((id, tx.clone()));
        (id, tx, rx)
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

fn conns_key(user_id: Uuid) -> String {
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
    // hash-field TTLs): a node that dies takes its sockets'"'"' fields with it
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
    }

    let (mut sink, mut stream) = socket.split();
    let _ = sink
        .send(WsMessage::Text(r#"{"type":"hello"}"#.to_string().into()))
        .await;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            outgoing = rx.recv() => match outgoing {
                Some(text) => {
                    if sink.send(WsMessage::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            incoming = stream.next() => match incoming {
                Some(Ok(WsMessage::Text(text))) if text.as_str() == "ping" => {
                    if sink.send(WsMessage::Text("pong".to_string().into())).await.is_err() {
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
                            let _ = tx.send(handle_request(&state, user_id, request).await.to_string());
                        });
                    }
                }
                Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
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
                if let Err(error) = crate::routes::rooms::leave_current_room(&state, user_id).await {
                    warn!(?error, %user_id, "failed to remove disconnected player from room");
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
        } => match crate::chat::send(state, user_id, &kind, target_id, &body).await {
            Ok(message) => serde_json::json!({ "type": "chat.ack", "requestId": request_id, "message": message }),
            Err(error) => serde_json::json!({
                "type": "chat.error", "requestId": request_id, "error": error.code, "message": error.message
            }),
        },
    }
}
