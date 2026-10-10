use crate::client_session::{ClientRegistry, ClientSink};
use crate::memberships::RoomMemberships;
use crate::room::{ClientId, RoomId};
use crate::room_registry::RoomRegistry;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};
use warlay_protocol::{ClientMessage, RoomMember, RoomStatus, ServerMessage};

/// Chosen generously for a casual chat channel, not a hard protocol limit —
/// just enough to stop one client from flooding everyone else's screen with
/// a single message.
const MAX_CHAT_MESSAGE_LEN: usize = 500;

/// Application-layer orchestrator for hub mode: the only place that
/// interprets `ClientMessage`s, mutates room/client state, and decides what
/// to broadcast. Framework-agnostic on purpose — it depends only on the
/// `ClientSink` trait, never on axum or WebSocket types, so it is trivially
/// unit-testable and the transport layer can be swapped without touching it.
pub struct MatchmakingHub {
    rooms: RoomRegistry,
    clients: ClientRegistry,
    memberships: Arc<RoomMemberships>,
    /// Rooms that have had at least one real captured W3GS packet relayed
    /// through them — lets `handle_relay_lan_packet` log the *first* one
    /// per room at `info` level (a genuinely useful "this is really
    /// working end to end" signal) without spamming the log on every
    /// subsequent broadcast WC3 sends every second or two.
    rooms_with_lan_relay_seen: Mutex<HashSet<RoomId>>,
}

impl MatchmakingHub {
    pub fn new(memberships: Arc<RoomMemberships>) -> Self {
        Self {
            rooms: RoomRegistry::default(),
            clients: ClientRegistry::default(),
            memberships,
            rooms_with_lan_relay_seen: Mutex::new(HashSet::new()),
        }
    }

    pub fn register_client(&self, client_id: ClientId, sink: Arc<dyn ClientSink>) {
        self.clients.register(client_id, sink);
        self.broadcast_server_stats();
    }

    pub fn unregister_client(&self, client_id: ClientId) {
        let Some((hosted_room, joined_room)) = self.clients.unregister(client_id) else {
            return;
        };
        // Disconnecting — however it happens, a clean `LeaveRoom`, a crash,
        // or a dropped connection — never deletes a room, even the host's:
        // the room stays listed and joinable by others already in it, and
        // is only ever removed by its creator's explicit `DeleteRoom` (see
        // `handle_delete_room`). This also covers the host's own socket
        // dying, which used to nuke the room out from under everyone else
        // still connected to it.
        if let Some(room_id) = hosted_room.or(joined_room) {
            self.note_participant_left(room_id, client_id);
        }
        self.broadcast_server_stats();
    }

    pub fn handle_message(&self, client_id: ClientId, message: ClientMessage) {
        match message {
            ClientMessage::Authenticate { display_name } => self.handle_authenticate(client_id, display_name),
            ClientMessage::CreateRoom {
                name,
                game_port,
                password,
            } => self.handle_create_room(client_id, name, game_port, password),
            ClientMessage::JoinRoom { room_id, password } => self.handle_join_room(client_id, room_id, password),
            ClientMessage::LeaveRoom => self.handle_leave_room(client_id),
            ClientMessage::DeleteRoom { room_id } => self.handle_delete_room(client_id, room_id),
            ClientMessage::RelayLanPacket { bytes } => self.handle_relay_lan_packet(client_id, bytes),
            ClientMessage::SetRoomInProgress => self.handle_set_in_progress(client_id),
            ClientMessage::SetRoomAvailable => self.handle_set_available(client_id),
            ClientMessage::ChatMessage { text } => self.handle_chat_message(client_id, text),
            ClientMessage::PrivateMessage { to_client_id, text } => {
                self.handle_private_message(client_id, to_client_id, text)
            }
            ClientMessage::Ping => self.clients.send_to(client_id, ServerMessage::Pong),
        }
    }

    fn broadcast_server_stats(&self) {
        self.clients.broadcast(ServerMessage::ServerStats {
            connected_clients: self.clients.connected_count(),
        });
    }

    fn require_authenticated(&self, client_id: ClientId) -> bool {
        if self.clients.is_authenticated(client_id) {
            return true;
        }
        self.clients.send_to(
            client_id,
            ServerMessage::Error {
                message: "debes autenticarte antes de continuar".to_string(),
            },
        );
        false
    }

    fn broadcast_room_list(&self) {
        let rooms = self.rooms.list().iter().map(crate::room::Room::to_summary).collect();
        self.clients.broadcast(ServerMessage::RoomList { rooms });
    }

    fn broadcast_room_members(&self, room_id: RoomId) {
        let Some(room) = self.rooms.get(room_id) else {
            return;
        };
        let members = room
            .members
            .iter()
            .map(|&client_id| RoomMember {
                client_id,
                display_name: self
                    .clients
                    .display_name(client_id)
                    .unwrap_or_else(|| "Jugador".to_string()),
            })
            .collect();
        self.clients
            .broadcast(ServerMessage::RoomMembersChanged { room_id, members });
    }

    fn handle_authenticate(&self, client_id: ClientId, display_name: String) {
        let display_name = display_name.trim();
        if display_name.is_empty() {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: "el nombre no puede estar vacío".to_string(),
                },
            );
            return;
        }

        let display_name = display_name.to_string();
        self.clients.set_display_name(client_id, display_name.clone());
        self.clients.send_to(
            client_id,
            ServerMessage::Authenticated {
                client_id,
                display_name,
            },
        );

        let rooms = self.rooms.list().iter().map(crate::room::Room::to_summary).collect();
        self.clients.send_to(client_id, ServerMessage::RoomList { rooms });
    }

    fn handle_create_room(&self, client_id: ClientId, name: String, game_port: u16, password: Option<String>) {
        if !self.require_authenticated(client_id) {
            return;
        }
        let name = name.trim();
        if name.is_empty() {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: "el nombre de la sala no puede estar vacío".to_string(),
                },
            );
            return;
        }

        if let Some(existing_room) = self.clients.hosted_room(client_id) {
            self.remove_hosted_room(existing_room);
        }

        let host_display_name = self
            .clients
            .display_name(client_id)
            .unwrap_or_else(|| "Jugador".to_string());
        let room = self
            .rooms
            .create(name.to_string(), client_id, host_display_name, game_port, password);
        self.rooms.add_member(room.id, client_id);
        self.clients.set_hosted_room(client_id, Some(room.id));

        info!(room_id = %room.id, room_name = %room.name, has_password = room.has_password(), "room created");

        self.clients.send_to(
            client_id,
            ServerMessage::RoomCreated {
                room: room.to_summary(),
                relay_token: room.id,
            },
        );
        self.broadcast_room_members(room.id);
        self.broadcast_room_list();
    }

    fn handle_join_room(&self, client_id: ClientId, room_id: RoomId, password: Option<String>) {
        if !self.require_authenticated(client_id) {
            return;
        }

        let Some(room) = self.rooms.get(room_id) else {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: "la sala ya no existe".to_string(),
                },
            );
            return;
        };
        if !room.password_matches(password.as_deref()) {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: "contraseña de sala incorrecta".to_string(),
                },
            );
            return;
        }

        let Some(room) = self.rooms.adjust_player_count(room_id, 1) else {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: "la sala ya no existe".to_string(),
                },
            );
            return;
        };
        self.rooms.add_member(room_id, client_id);

        self.clients.set_joined_room(client_id, Some(room_id));
        info!(room_id = %room_id, client_id = %client_id, "client joined room");

        self.clients.send_to(
            client_id,
            ServerMessage::JoinedRoom {
                room: room.to_summary(),
                relay_token: room.id,
            },
        );
        self.clients.broadcast(ServerMessage::RoomUpdated {
            room: room.to_summary(),
        });
        self.broadcast_room_members(room_id);
    }

    /// Steps a client back from a room — host or not — without deleting it.
    /// A host who leaves this way can rejoin (or someone else can host their
    /// own game) later; the room only ever disappears via `DeleteRoom`.
    fn handle_leave_room(&self, client_id: ClientId) {
        if let Some(room_id) = self.clients.hosted_room(client_id) {
            self.clients.set_hosted_room(client_id, None);
            self.note_participant_left(room_id, client_id);
            self.clients.send_to(client_id, ServerMessage::LeftRoom);
        } else if let Some(room_id) = self.clients.joined_room(client_id) {
            self.clients.set_joined_room(client_id, None);
            self.note_participant_left(room_id, client_id);
            self.clients.send_to(client_id, ServerMessage::LeftRoom);
        } else {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: "no estás en ninguna sala".to_string(),
                },
            );
        }
    }

    /// Permanently closes the room — the only thing that actually removes
    /// it. Restricted to the room's original creator (`host_client_id`,
    /// which is the caller's persistent identity and so survives them
    /// reconnecting), so a participant merely disconnecting or leaving can
    /// never be mistaken for this.
    fn handle_delete_room(&self, client_id: ClientId, room_id: RoomId) {
        if !self.require_authenticated(client_id) {
            return;
        }
        let Some(room) = self.rooms.get(room_id) else {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: "la sala ya no existe".to_string(),
                },
            );
            return;
        };
        if room.host_client_id != client_id {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: "solo quien creó la sala puede eliminarla".to_string(),
                },
            );
            return;
        }
        self.remove_hosted_room(room_id);
    }

    /// Fans a real, captured W3GS broadcast out to the rest of the sender's
    /// room. Silently dropped if the sender isn't currently in one — this
    /// rides on a background capture loop, not a user action, so there's no
    /// one waiting on an `Error` reply for it.
    fn handle_relay_lan_packet(&self, client_id: ClientId, bytes: Vec<u8>) {
        let Some(room_id) = self
            .clients
            .hosted_room(client_id)
            .or_else(|| self.clients.joined_room(client_id))
        else {
            return;
        };

        let is_first_for_room = self
            .rooms_with_lan_relay_seen
            .lock()
            .expect("lan relay seen-set lock poisoned")
            .insert(room_id);
        if is_first_for_room {
            info!(
                room_id = %room_id,
                bytes = bytes.len(),
                "relaying this room's first real captured W3GS packet — the LAN capture -> relay -> rebroadcast \
                 pipeline is confirmed working end to end for this room"
            );
        } else {
            debug!(room_id = %room_id, bytes = bytes.len(), "relaying a real W3GS packet");
        }

        self.clients
            .send_to_room_except(room_id, client_id, ServerMessage::LanPacket { bytes });
    }

    fn handle_set_in_progress(&self, client_id: ClientId) {
        let Some(room_id) = self.clients.hosted_room(client_id) else {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: "solo el anfitrión puede cambiar el estado de la sala".to_string(),
                },
            );
            return;
        };
        if let Some(room) = self.rooms.set_status(room_id, RoomStatus::InProgress) {
            self.clients.broadcast(ServerMessage::RoomUpdated {
                room: room.to_summary(),
            });
        }
    }

    /// Sibling of `handle_set_in_progress`, reverting a room back to open.
    /// Same host-only enforcement (`hosted_room` is the *current session's*
    /// host link, reset only by an explicit `LeaveRoom`/disconnect — not by
    /// the client's WC3 process merely exiting, which no longer sends one).
    fn handle_set_available(&self, client_id: ClientId) {
        let Some(room_id) = self.clients.hosted_room(client_id) else {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: "solo el anfitrión puede cambiar el estado de la sala".to_string(),
                },
            );
            return;
        };
        if let Some(room) = self.rooms.set_status(room_id, RoomStatus::Available) {
            self.clients.broadcast(ServerMessage::RoomUpdated {
                room: room.to_summary(),
            });
        }
    }

    /// One global channel per server — not per room — so any authenticated
    /// client reaches everyone currently connected, via the same
    /// `ClientRegistry::broadcast` every other server-wide push
    /// (`ServerStats`, `RoomList`) already uses. No history: a client that
    /// connects after a message was sent never sees it — simplest possible
    /// v1, matching there being no friends list or per-room scope yet.
    fn handle_chat_message(&self, client_id: ClientId, text: String) {
        if !self.require_authenticated(client_id) {
            return;
        }
        let text = text.trim();
        if text.is_empty() {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: "el mensaje no puede estar vacío".to_string(),
                },
            );
            return;
        }
        if text.chars().count() > MAX_CHAT_MESSAGE_LEN {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: format!("el mensaje supera los {MAX_CHAT_MESSAGE_LEN} caracteres"),
                },
            );
            return;
        }

        let from_display_name = self
            .clients
            .display_name(client_id)
            .unwrap_or_else(|| "Jugador".to_string());
        let unix_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);

        self.clients.broadcast(ServerMessage::ChatMessage {
            from_display_name,
            from_client_id: client_id,
            text: text.to_string(),
            unix_millis,
        });
    }

    fn handle_private_message(&self, client_id: ClientId, to_client_id: ClientId, text: String) {
        if !self.require_authenticated(client_id) {
            return;
        }
        let text = text.trim();
        if text.is_empty() {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: "el mensaje no puede estar vacío".to_string(),
                },
            );
            return;
        }
        if text.chars().count() > MAX_CHAT_MESSAGE_LEN {
            self.clients.send_to(
                client_id,
                ServerMessage::Error {
                    message: format!("el mensaje supera los {MAX_CHAT_MESSAGE_LEN} caracteres"),
                },
            );
            return;
        }

        let from_display_name = self
            .clients
            .display_name(client_id)
            .unwrap_or_else(|| "Jugador".to_string());
        let unix_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);

        let pm = ServerMessage::PrivateMessage {
            from_display_name,
            from_client_id: client_id,
            to_client_id,
            text: text.to_string(),
            unix_millis,
        };

        // Send to the recipient
        self.clients.send_to(to_client_id, pm.clone());

        // Also send back to the sender so their UI can confirm it was sent
        if client_id != to_client_id {
            self.clients.send_to(client_id, pm);
        }
    }

    fn remove_hosted_room(&self, room_id: RoomId) {
        if self.rooms.remove(room_id).is_some() {
            self.memberships.drop_room(room_id);
            self.rooms_with_lan_relay_seen
                .lock()
                .expect("lan relay seen-set lock poisoned")
                .remove(&room_id);
            info!(room_id = %room_id, "room removed");
            self.clients.broadcast(ServerMessage::RoomRemoved { room_id });
        }
    }

    /// Removes every room that's had zero connected participants for at
    /// least `max_empty_secs` — called periodically from `main.rs`, not in
    /// response to any single client action. Returns how many were purged,
    /// purely for logging.
    pub fn purge_inactive_rooms(&self, max_empty_secs: u64) -> usize {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let cutoff = now.saturating_sub(max_empty_secs);
        let stale = self.rooms.empty_since_before(cutoff);
        for room_id in &stale {
            self.remove_hosted_room(*room_id);
        }
        stale.len()
    }

    /// A host or joiner stepped away (disconnect, crash, or explicit
    /// `LeaveRoom`) without deleting the room: reflect one fewer connected
    /// participant and broadcast the update, but leave the room listed.
    fn note_participant_left(&self, room_id: RoomId, client_id: ClientId) {
        self.rooms.remove_member(room_id, client_id);
        if let Some(room) = self.rooms.adjust_player_count(room_id, -1) {
            self.clients.broadcast(ServerMessage::RoomUpdated {
                room: room.to_summary(),
            });
            self.broadcast_room_members(room_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        messages: Mutex<Vec<ServerMessage>>,
    }

    impl ClientSink for RecordingSink {
        fn send(&self, message: ServerMessage) {
            self.messages.lock().unwrap().push(message);
        }
    }

    fn register(hub: &MatchmakingHub) -> (ClientId, Arc<RecordingSink>) {
        let client_id = ClientId::new_v4();
        let sink = Arc::new(RecordingSink::default());
        hub.register_client(client_id, sink.clone());
        (client_id, sink)
    }

    #[test]
    fn room_members_changed_emitted_on_create_join_leave_and_disconnect() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (host_id, host_sink) = register(&hub);
        hub.handle_message(
            host_id,
            ClientMessage::Authenticate {
                display_name: "Host".to_string(),
            },
        );
        hub.handle_message(
            host_id,
            ClientMessage::CreateRoom {
                name: "Sala test".to_string(),
                game_port: 6112,
                password: None,
            },
        );

        // 1. Host creates room -> receives RoomMembersChanged containing Host
        let room_id = {
            let messages = host_sink.messages.lock().unwrap();
            let members_msg = messages.iter().find_map(|m| match m {
                ServerMessage::RoomMembersChanged { room_id, members } => Some((*room_id, members.clone())),
                _ => None,
            });
            assert!(members_msg.is_some());
            let (r_id, members) = members_msg.unwrap();
            assert_eq!(members.len(), 1);
            assert_eq!(members[0].client_id, host_id);
            assert_eq!(members[0].display_name, "Host");
            r_id
        };

        // 2. Joiner authenticates & joins -> both receive RoomMembersChanged with 2 members
        let (joiner_id, joiner_sink) = register(&hub);
        hub.handle_message(
            joiner_id,
            ClientMessage::Authenticate {
                display_name: "Joiner".to_string(),
            },
        );
        hub.handle_message(
            joiner_id,
            ClientMessage::JoinRoom {
                room_id,
                password: None,
            },
        );

        {
            let messages = joiner_sink.messages.lock().unwrap();
            let last_members_msg = messages
                .iter()
                .filter_map(|m| match m {
                    ServerMessage::RoomMembersChanged { room_id: r_id, members } if *r_id == room_id => {
                        Some(members.clone())
                    }
                    _ => None,
                })
                .last();
            assert!(last_members_msg.is_some());
            let members = last_members_msg.unwrap();
            assert_eq!(members.len(), 2);
            assert!(members
                .iter()
                .any(|m| m.client_id == host_id && m.display_name == "Host"));
            assert!(members
                .iter()
                .any(|m| m.client_id == joiner_id && m.display_name == "Joiner"));
        }

        // 3. Joiner leaves explicitly -> RoomMembersChanged has only Host
        hub.handle_message(joiner_id, ClientMessage::LeaveRoom);
        {
            let messages = host_sink.messages.lock().unwrap();
            let last_members_msg = messages
                .iter()
                .filter_map(|m| match m {
                    ServerMessage::RoomMembersChanged { room_id: r_id, members } if *r_id == room_id => {
                        Some(members.clone())
                    }
                    _ => None,
                })
                .last();
            assert!(last_members_msg.is_some());
            let members = last_members_msg.unwrap();
            assert_eq!(members.len(), 1);
            assert_eq!(members[0].client_id, host_id);
        }

        // 4. Client2 joins and disconnects (unregister) -> Host receives update removing Client2
        let (c2_id, _c2_sink) = register(&hub);
        hub.handle_message(
            c2_id,
            ClientMessage::Authenticate {
                display_name: "Client2".to_string(),
            },
        );
        hub.handle_message(
            c2_id,
            ClientMessage::JoinRoom {
                room_id,
                password: None,
            },
        );

        hub.unregister_client(c2_id);
        {
            let messages = host_sink.messages.lock().unwrap();
            let last_members_msg = messages
                .iter()
                .filter_map(|m| match m {
                    ServerMessage::RoomMembersChanged { room_id: r_id, members } if *r_id == room_id => {
                        Some(members.clone())
                    }
                    _ => None,
                })
                .last();
            assert!(last_members_msg.is_some());
            let members = last_members_msg.unwrap();
            assert_eq!(members.len(), 1);
            assert_eq!(members[0].client_id, host_id);
        }
    }

    #[test]
    fn create_room_acknowledges_creator_and_broadcasts_list() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (host_id, host_sink) = register(&hub);
        hub.handle_message(
            host_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );

        hub.handle_message(
            host_id,
            ClientMessage::CreateRoom {
                name: "Sala de prueba".to_string(),
                game_port: 6112,
                password: None,
            },
        );

        let messages = host_sink.messages.lock().unwrap();
        assert!(messages.iter().any(|m| matches!(m, ServerMessage::RoomCreated { .. })));
        assert!(messages
            .iter()
            .any(|m| matches!(m, ServerMessage::RoomList { rooms } if rooms.len() == 1)));
    }

    /// Reproduces a real bug report: a second machine ("laptop"), already
    /// connected and authenticated *before* the first ("PC") founds a room,
    /// should still see it show up — the hub must broadcast the updated
    /// room list to every authenticated client, not just echo it back to
    /// the creator. (The actual field failure was a client-side issue —
    /// the laptop's `Authenticate` silently failed on an empty display
    /// name, so it never got this far — but the hub's own fan-out behavior
    /// is worth pinning down directly.)
    #[test]
    fn a_room_created_by_one_client_reaches_another_already_connected_client() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (laptop_id, laptop_sink) = register(&hub);
        hub.handle_message(
            laptop_id,
            ClientMessage::Authenticate {
                display_name: "Laptop".to_string(),
            },
        );

        let (pc_id, _pc_sink) = register(&hub);
        hub.handle_message(
            pc_id,
            ClientMessage::Authenticate {
                display_name: "PC".to_string(),
            },
        );
        hub.handle_message(
            pc_id,
            ClientMessage::CreateRoom {
                name: "Sala del PC".to_string(),
                game_port: 6112,
                password: None,
            },
        );

        let messages = laptop_sink.messages.lock().unwrap();
        let saw_the_room = messages
            .iter()
            .any(|m| matches!(m, ServerMessage::RoomList { rooms } if rooms.iter().any(|r| r.name == "Sala del PC")));
        assert!(
            saw_the_room,
            "laptop should have received the PC's room via the RoomList broadcast"
        );
    }

    #[test]
    fn unauthenticated_create_room_is_rejected() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (client_id, sink) = register(&hub);

        hub.handle_message(
            client_id,
            ClientMessage::CreateRoom {
                name: "Sala".to_string(),
                game_port: 6112,
                password: None,
            },
        );

        let messages = sink.messages.lock().unwrap();
        assert!(matches!(messages.last(), Some(ServerMessage::Error { .. })));
    }

    #[test]
    fn relayed_lan_packet_reaches_the_rest_of_the_room_but_not_the_sender() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (host_id, host_sink) = register(&hub);
        hub.handle_message(
            host_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );
        hub.handle_message(
            host_id,
            ClientMessage::CreateRoom {
                name: "Sala".to_string(),
                game_port: 6112,
                password: None,
            },
        );

        let (joiner_id, joiner_sink) = register(&hub);
        hub.handle_message(
            joiner_id,
            ClientMessage::Authenticate {
                display_name: "Jaina".to_string(),
            },
        );
        let room_id = joiner_sink
            .messages
            .lock()
            .unwrap()
            .iter()
            .find_map(|m| match m {
                ServerMessage::RoomList { rooms } if !rooms.is_empty() => Some(rooms[0].id),
                _ => None,
            })
            .unwrap();
        hub.handle_message(
            joiner_id,
            ClientMessage::JoinRoom {
                room_id,
                password: None,
            },
        );

        let fake_packet = vec![0xF7, 0x30, 0x10, 0x00, 1, 2, 3, 4];
        hub.handle_message(
            host_id,
            ClientMessage::RelayLanPacket {
                bytes: fake_packet.clone(),
            },
        );

        let joiner_messages = joiner_sink.messages.lock().unwrap();
        assert!(joiner_messages
            .iter()
            .any(|m| matches!(m, ServerMessage::LanPacket { bytes } if *bytes == fake_packet)));

        let host_messages = host_sink.messages.lock().unwrap();
        assert!(
            !host_messages
                .iter()
                .any(|m| matches!(m, ServerMessage::LanPacket { .. })),
            "the relay must not echo the packet back to whoever sent it"
        );
    }

    #[test]
    fn join_room_increments_player_count_and_notifies_joiner() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (host_id, _) = register(&hub);
        hub.handle_message(
            host_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );
        hub.handle_message(
            host_id,
            ClientMessage::CreateRoom {
                name: "Sala".to_string(),
                game_port: 6112,
                password: None,
            },
        );

        let (joiner_id, joiner_sink) = register(&hub);
        hub.handle_message(
            joiner_id,
            ClientMessage::Authenticate {
                display_name: "Jaina".to_string(),
            },
        );

        let messages = joiner_sink.messages.lock().unwrap();
        let room_id = messages
            .iter()
            .find_map(|m| match m {
                ServerMessage::RoomList { rooms } if !rooms.is_empty() => Some(rooms[0].id),
                _ => None,
            })
            .expect("joiner should have received the room list");
        drop(messages);

        hub.handle_message(
            joiner_id,
            ClientMessage::JoinRoom {
                room_id,
                password: None,
            },
        );

        let messages = joiner_sink.messages.lock().unwrap();
        let joined = messages.iter().find_map(|m| match m {
            ServerMessage::JoinedRoom { room, .. } => Some(room.clone()),
            _ => None,
        });
        assert_eq!(joined.map(|room| room.player_count), Some(2));
    }

    #[test]
    fn disconnecting_host_keeps_the_room_listed_for_everyone_else() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (host_id, _) = register(&hub);
        hub.handle_message(
            host_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );
        hub.handle_message(
            host_id,
            ClientMessage::CreateRoom {
                name: "Sala".to_string(),
                game_port: 6112,
                password: None,
            },
        );

        let (observer_id, observer_sink) = register(&hub);
        hub.handle_message(
            observer_id,
            ClientMessage::Authenticate {
                display_name: "Observer".to_string(),
            },
        );

        hub.unregister_client(host_id);

        let messages = observer_sink.messages.lock().unwrap();
        assert!(
            !messages.iter().any(|m| matches!(m, ServerMessage::RoomRemoved { .. })),
            "the host disconnecting must not delete the room"
        );
        assert!(messages
            .iter()
            .any(|m| matches!(m, ServerMessage::RoomUpdated { room } if room.player_count == 0)));
    }

    #[test]
    fn leaving_as_host_does_not_delete_the_room_either() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (host_id, host_sink) = register(&hub);
        hub.handle_message(
            host_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );
        hub.handle_message(
            host_id,
            ClientMessage::CreateRoom {
                name: "Sala".to_string(),
                game_port: 6112,
                password: None,
            },
        );

        hub.handle_message(host_id, ClientMessage::LeaveRoom);

        let messages = host_sink.messages.lock().unwrap();
        assert!(messages.iter().any(|m| matches!(m, ServerMessage::LeftRoom)));
        assert!(!messages.iter().any(|m| matches!(m, ServerMessage::RoomRemoved { .. })));
    }

    #[test]
    fn only_the_room_creator_can_delete_it() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (host_id, _) = register(&hub);
        hub.handle_message(
            host_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );
        hub.handle_message(
            host_id,
            ClientMessage::CreateRoom {
                name: "Sala".to_string(),
                game_port: 6112,
                password: None,
            },
        );

        let (other_id, other_sink) = register(&hub);
        hub.handle_message(
            other_id,
            ClientMessage::Authenticate {
                display_name: "Jaina".to_string(),
            },
        );
        let room_id = other_sink
            .messages
            .lock()
            .unwrap()
            .iter()
            .find_map(|m| match m {
                ServerMessage::RoomList { rooms } if !rooms.is_empty() => Some(rooms[0].id),
                _ => None,
            })
            .expect("should have received the room list");

        hub.handle_message(other_id, ClientMessage::DeleteRoom { room_id });
        assert!(matches!(
            other_sink.messages.lock().unwrap().last(),
            Some(ServerMessage::Error { .. })
        ));

        hub.handle_message(host_id, ClientMessage::DeleteRoom { room_id });
        let messages = other_sink.messages.lock().unwrap();
        assert!(messages.iter().any(|m| matches!(m, ServerMessage::RoomRemoved { .. })));
    }

    #[test]
    fn the_creator_can_still_delete_the_room_after_reconnecting() {
        // host_client_id is the caller's persistent identity, so ownership
        // survives a disconnect + fresh session under the same client_id —
        // unlike `ClientRegistry`'s per-session `hosted_room` link, which is
        // reset on every reconnect.
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let host_id = ClientId::new_v4();
        let sink = Arc::new(RecordingSink::default());
        hub.register_client(host_id, sink.clone());
        hub.handle_message(
            host_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );
        hub.handle_message(
            host_id,
            ClientMessage::CreateRoom {
                name: "Sala".to_string(),
                game_port: 6112,
                password: None,
            },
        );
        let room_id = sink
            .messages
            .lock()
            .unwrap()
            .iter()
            .find_map(|m| match m {
                ServerMessage::RoomCreated { room, .. } => Some(room.id),
                _ => None,
            })
            .unwrap();

        hub.unregister_client(host_id);
        // Same persistent client_id, brand-new session.
        let new_sink = Arc::new(RecordingSink::default());
        hub.register_client(host_id, new_sink.clone());
        hub.handle_message(
            host_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );

        hub.handle_message(host_id, ClientMessage::DeleteRoom { room_id });
        assert!(new_sink
            .messages
            .lock()
            .unwrap()
            .iter()
            .any(|m| matches!(m, ServerMessage::RoomRemoved { .. })));
    }

    #[test]
    fn joining_a_password_protected_room_requires_the_right_password() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (host_id, _) = register(&hub);
        hub.handle_message(
            host_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );
        hub.handle_message(
            host_id,
            ClientMessage::CreateRoom {
                name: "Sala privada".to_string(),
                game_port: 6112,
                password: Some("secreta".to_string()),
            },
        );

        let (joiner_id, joiner_sink) = register(&hub);
        hub.handle_message(
            joiner_id,
            ClientMessage::Authenticate {
                display_name: "Jaina".to_string(),
            },
        );
        let room_id = joiner_sink
            .messages
            .lock()
            .unwrap()
            .iter()
            .find_map(|m| match m {
                ServerMessage::RoomList { rooms } if !rooms.is_empty() => Some(rooms[0].id),
                _ => None,
            })
            .expect("joiner should have received the room list");

        hub.handle_message(
            joiner_id,
            ClientMessage::JoinRoom {
                room_id,
                password: Some("incorrecta".to_string()),
            },
        );
        assert!(matches!(
            joiner_sink.messages.lock().unwrap().last(),
            Some(ServerMessage::Error { .. })
        ));

        hub.handle_message(
            joiner_id,
            ClientMessage::JoinRoom {
                room_id,
                password: Some("secreta".to_string()),
            },
        );
        assert!(joiner_sink
            .messages
            .lock()
            .unwrap()
            .iter()
            .any(|m| matches!(m, ServerMessage::JoinedRoom { .. })));
    }

    #[test]
    fn host_can_flip_room_status_to_in_progress_and_back_to_available() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (host_id, host_sink) = register(&hub);
        hub.handle_message(
            host_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );
        hub.handle_message(
            host_id,
            ClientMessage::CreateRoom {
                name: "Sala".to_string(),
                game_port: 6112,
                password: None,
            },
        );

        hub.handle_message(host_id, ClientMessage::SetRoomInProgress);
        assert!(host_sink.messages.lock().unwrap().iter().any(
            |m| matches!(m, ServerMessage::RoomUpdated { room } if matches!(room.status, RoomStatus::InProgress))
        ));

        hub.handle_message(host_id, ClientMessage::SetRoomAvailable);
        assert!(host_sink
            .messages
            .lock()
            .unwrap()
            .iter()
            .any(|m| matches!(m, ServerMessage::RoomUpdated { room } if matches!(room.status, RoomStatus::Available))));
    }

    #[test]
    fn only_the_host_can_set_a_room_available() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (host_id, _) = register(&hub);
        hub.handle_message(
            host_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );
        hub.handle_message(
            host_id,
            ClientMessage::CreateRoom {
                name: "Sala".to_string(),
                game_port: 6112,
                password: None,
            },
        );

        let (other_id, other_sink) = register(&hub);
        hub.handle_message(
            other_id,
            ClientMessage::Authenticate {
                display_name: "Jaina".to_string(),
            },
        );

        hub.handle_message(other_id, ClientMessage::SetRoomAvailable);
        assert!(matches!(
            other_sink.messages.lock().unwrap().last(),
            Some(ServerMessage::Error { .. })
        ));
    }

    #[test]
    fn chat_message_reaches_every_connected_client_including_the_sender() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (sender_id, sender_sink) = register(&hub);
        hub.handle_message(
            sender_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );

        let (other_id, other_sink) = register(&hub);
        hub.handle_message(
            other_id,
            ClientMessage::Authenticate {
                display_name: "Jaina".to_string(),
            },
        );

        hub.handle_message(
            sender_id,
            ClientMessage::ChatMessage {
                text: "hola a todos".to_string(),
            },
        );

        for sink in [&sender_sink, &other_sink] {
            let messages = sink.messages.lock().unwrap();
            let chat = messages.iter().find_map(|m| match m {
                ServerMessage::ChatMessage {
                    from_display_name,
                    text,
                    ..
                } => Some((from_display_name.clone(), text.clone())),
                _ => None,
            });
            assert_eq!(chat, Some(("Thrall".to_string(), "hola a todos".to_string())));
        }
    }

    #[test]
    fn unauthenticated_chat_message_is_rejected() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (client_id, sink) = register(&hub);

        hub.handle_message(
            client_id,
            ClientMessage::ChatMessage {
                text: "hola".to_string(),
            },
        );

        assert!(matches!(
            sink.messages.lock().unwrap().last(),
            Some(ServerMessage::Error { .. })
        ));
    }

    #[test]
    fn empty_or_oversized_chat_messages_are_rejected() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (client_id, sink) = register(&hub);
        hub.handle_message(
            client_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );

        hub.handle_message(
            client_id,
            ClientMessage::ChatMessage {
                text: "   ".to_string(),
            },
        );
        assert!(matches!(
            sink.messages.lock().unwrap().last(),
            Some(ServerMessage::Error { .. })
        ));

        hub.handle_message(client_id, ClientMessage::ChatMessage { text: "x".repeat(501) });
        assert!(matches!(
            sink.messages.lock().unwrap().last(),
            Some(ServerMessage::Error { .. })
        ));
    }

    #[test]
    fn registering_a_client_broadcasts_updated_server_stats() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));
        let (first_id, first_sink) = register(&hub);
        hub.handle_message(
            first_id,
            ClientMessage::Authenticate {
                display_name: "Thrall".to_string(),
            },
        );

        let (_second_id, _second_sink) = register(&hub);

        let messages = first_sink.messages.lock().unwrap();
        let last_count = messages.iter().rev().find_map(|m| match m {
            ServerMessage::ServerStats { connected_clients } => Some(*connected_clients),
            _ => None,
        });
        assert_eq!(last_count, Some(2));
    }

    #[test]
    fn purge_inactive_rooms_removes_only_the_ones_that_are_actually_empty() {
        let hub = MatchmakingHub::new(Arc::new(RoomMemberships::default()));

        let (empty_host_id, _) = register(&hub);
        hub.handle_message(
            empty_host_id,
            ClientMessage::Authenticate {
                display_name: "Solo".to_string(),
            },
        );
        hub.handle_message(
            empty_host_id,
            ClientMessage::CreateRoom {
                name: "Se vació".to_string(),
                game_port: 6112,
                password: None,
            },
        );
        hub.unregister_client(empty_host_id); // drops it to 0 players, starting the empty timer

        let (busy_host_id, busy_sink) = register(&hub);
        hub.handle_message(
            busy_host_id,
            ClientMessage::Authenticate {
                display_name: "Sigue ahí".to_string(),
            },
        );
        hub.handle_message(
            busy_host_id,
            ClientMessage::CreateRoom {
                name: "Sigue activa".to_string(),
                game_port: 6113,
                password: None,
            },
        );

        // Real wall-clock gap so the empty room's `empty_since` timestamp
        // (whole unix seconds) is unambiguously in the past relative to the
        // purge cutoff computed below.
        std::thread::sleep(std::time::Duration::from_millis(1100));

        let purged = hub.purge_inactive_rooms(0);
        assert_eq!(purged, 1, "only the empty room should have been purged");

        let remaining: Vec<_> = hub.rooms.list();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].name, "Sigue activa");

        // RoomRemoved is a global broadcast (every connected client's room
        // list needs the purged entry gone, not just that room's members),
        // so the busy client legitimately hears about it too — what
        // matters is that its *own* room survives untouched, which the
        // `remaining` check above already confirms.
        let busy_messages = busy_sink.messages.lock().unwrap();
        assert!(busy_messages
            .iter()
            .any(|m| matches!(m, ServerMessage::RoomRemoved { room_id } if *room_id != remaining[0].id)));
    }
}
