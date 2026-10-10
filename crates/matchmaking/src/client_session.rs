use crate::room::{ClientId, RoomId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use warlay_protocol::ServerMessage;

/// Decouples `MatchmakingHub` from whatever transport actually delivers a
/// message to a connected client. The binary crate provides the real
/// implementation (an `mpsc::UnboundedSender` feeding an axum WebSocket
/// write task); this crate never has to know axum or WebSocket exist.
pub trait ClientSink: Send + Sync {
    fn send(&self, message: ServerMessage);
}

struct ClientSession {
    sink: Arc<dyn ClientSink>,
    display_name: Option<String>,
    hosted_room: Option<RoomId>,
    joined_room: Option<RoomId>,
}

/// Everything the hub knows about currently connected clients: how to reach
/// them, whether they've authenticated, and which room (if any) they're
/// hosting or have joined.
#[derive(Default)]
pub struct ClientRegistry {
    sessions: RwLock<HashMap<ClientId, ClientSession>>,
}

impl ClientRegistry {
    pub fn register(&self, client_id: ClientId, sink: Arc<dyn ClientSink>) {
        self.sessions.write().expect("client registry lock poisoned").insert(
            client_id,
            ClientSession {
                sink,
                display_name: None,
                hosted_room: None,
                joined_room: None,
            },
        );
    }

    /// Removes the session, returning its last known room memberships so the
    /// caller can clean up any room the client was hosting or had joined.
    pub fn unregister(&self, client_id: ClientId) -> Option<(Option<RoomId>, Option<RoomId>)> {
        self.sessions
            .write()
            .expect("client registry lock poisoned")
            .remove(&client_id)
            .map(|session| (session.hosted_room, session.joined_room))
    }

    pub fn set_display_name(&self, client_id: ClientId, display_name: String) {
        if let Some(session) = self
            .sessions
            .write()
            .expect("client registry lock poisoned")
            .get_mut(&client_id)
        {
            session.display_name = Some(display_name);
        }
    }

    pub fn display_name(&self, client_id: ClientId) -> Option<String> {
        self.sessions
            .read()
            .expect("client registry lock poisoned")
            .get(&client_id)?
            .display_name
            .clone()
    }

    pub fn is_authenticated(&self, client_id: ClientId) -> bool {
        self.display_name(client_id).is_some()
    }

    pub fn set_hosted_room(&self, client_id: ClientId, room_id: Option<RoomId>) {
        if let Some(session) = self
            .sessions
            .write()
            .expect("client registry lock poisoned")
            .get_mut(&client_id)
        {
            session.hosted_room = room_id;
        }
    }

    pub fn set_joined_room(&self, client_id: ClientId, room_id: Option<RoomId>) {
        if let Some(session) = self
            .sessions
            .write()
            .expect("client registry lock poisoned")
            .get_mut(&client_id)
        {
            session.joined_room = room_id;
        }
    }

    pub fn hosted_room(&self, client_id: ClientId) -> Option<RoomId> {
        self.sessions
            .read()
            .expect("client registry lock poisoned")
            .get(&client_id)?
            .hosted_room
    }

    pub fn joined_room(&self, client_id: ClientId) -> Option<RoomId> {
        self.sessions
            .read()
            .expect("client registry lock poisoned")
            .get(&client_id)?
            .joined_room
    }

    pub fn send_to(&self, client_id: ClientId, message: ServerMessage) {
        if let Some(session) = self
            .sessions
            .read()
            .expect("client registry lock poisoned")
            .get(&client_id)
        {
            session.sink.send(message);
        }
    }

    pub fn broadcast(&self, message: ServerMessage) {
        let sessions = self.sessions.read().expect("client registry lock poisoned");
        for session in sessions.values() {
            session.sink.send(message.clone());
        }
    }

    /// Sends to every client currently hosting or having joined `room_id`,
    /// except `exclude` — used to fan a `RelayLanPacket` out to the rest of
    /// a room without echoing it back to whoever sent it.
    pub fn send_to_room_except(&self, room_id: RoomId, exclude: ClientId, message: ServerMessage) {
        let sessions = self.sessions.read().expect("client registry lock poisoned");
        for (&client_id, session) in sessions.iter() {
            if client_id == exclude {
                continue;
            }
            let in_room = session.hosted_room == Some(room_id) || session.joined_room == Some(room_id);
            if in_room {
                session.sink.send(message.clone());
            }
        }
    }

    pub fn connected_count(&self) -> u32 {
        self.sessions.read().expect("client registry lock poisoned").len() as u32
    }
}
