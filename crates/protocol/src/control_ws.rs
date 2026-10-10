use crate::error::ProtocolError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// `Vec<u8>` fields serialize as JSON number arrays by default (`[247,48,...]`),
/// which is both bulky and awkward to read in logs — base64 keeps the raw
/// captured W3GS bytes as a single compact string instead.
mod base64_bytes {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        base64::engine::general_purpose::STANDARD
            .encode(bytes)
            .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let encoded = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(serde::de::Error::custom)
    }
}

/// Hub mode's WebSocket control-plane protocol between a Launcher client and
/// the centralized matchmaking server. JSON on the wire (not bincode): this
/// is a low-frequency, human-diagnosable control channel, unlike the
/// high-frequency binary UDP data plane (`RelayFrame`). `camelCase` so the
/// same struct can be reused verbatim as a Tauri IPC DTO on the client side
/// without a separate translation layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoomStatus {
    Available,
    InProgress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomMember {
    pub client_id: Uuid,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomSummary {
    pub id: Uuid,
    pub name: String,
    pub host_display_name: String,
    /// The room creator's persistent client identity (the same id across
    /// reconnects — see `HandshakeHello::client_id`), not a per-connection
    /// session id. Lets a client recognize "this is my room" — and so show
    /// a delete option — even after it reconnects and its active session
    /// state has been reset. Not sensitive: it's an opaque id, never PII.
    pub host_client_id: Uuid,
    pub game_port: u16,
    pub status: RoomStatus,
    pub player_count: u32,
    pub created_unix_secs: u64,
    /// Never the password itself — just whether `JoinRoom` needs one.
    pub has_password: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ClientMessage {
    Authenticate {
        display_name: String,
    },
    CreateRoom {
        name: String,
        game_port: u16,
        password: Option<String>,
    },
    JoinRoom {
        room_id: Uuid,
        password: Option<String>,
    },
    LeaveRoom,
    /// Permanently closes the room for everyone. Only the room's original
    /// creator may do this (see `host_client_id`) — anyone else leaves via
    /// `LeaveRoom`, which no longer deletes the room even for the host; see
    /// `MatchmakingHub::handle_leave_room`.
    DeleteRoom {
        room_id: Uuid,
    },
    /// The raw bytes of a real W3GS UDP broadcast this client's own
    /// Warcraft III just sent on its local network (captured off the wire —
    /// see the Launcher's `network::lan_capture` — not synthesized). The hub
    /// fans it out verbatim to every other client currently in the same
    /// room, who rebroadcast it unmodified on *their* own local network so
    /// their WC3 sees the real thing instead of a guessed reconstruction.
    RelayLanPacket {
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
    },
    SetRoomInProgress,
    /// Reverts a room back to `Available` — host-only, same enforcement as
    /// `SetRoomInProgress`. The daemon sends this automatically when it
    /// observes the host's WC3 re-announcing a game on the LAN (W3GS has no
    /// "match ended" packet, so a re-announced game is the practical proxy
    /// for "the previous session is over" — see `contains_create_game` in
    /// `crates/engine/src/w3gs_sniff.rs`), mirroring how `SetRoomInProgress`
    /// is triggered by observing `COUNTDOWN_START`.
    SetRoomAvailable,
    /// Global per-server chat — not scoped to a room. The hub stamps the
    /// sender's display name and a timestamp; `text` is the only thing the
    /// client sends.
    ChatMessage {
        text: String,
    },
    /// Private 1-on-1 chat. Routed by the server only to the target client.
    PrivateMessage {
        to_client_id: Uuid,
        text: String,
    },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ServerMessage {
    Authenticated {
        client_id: Uuid,
        display_name: String,
    },
    RoomList {
        rooms: Vec<RoomSummary>,
    },
    RoomCreated {
        room: RoomSummary,
        relay_token: Uuid,
    },
    RoomUpdated {
        room: RoomSummary,
    },
    RoomRemoved {
        room_id: Uuid,
    },
    JoinedRoom {
        room: RoomSummary,
        relay_token: Uuid,
    },
    LeftRoom,
    RoomMembersChanged {
        room_id: Uuid,
        members: Vec<RoomMember>,
    },
    /// How many clients currently hold an authenticated session on this
    /// server — pushed alongside `RoomList` whenever it changes, so the
    /// client's server list can show live "usuarios conectados".
    ServerStats {
        connected_clients: u32,
    },
    /// A real W3GS broadcast another member of this room's WC3 sent,
    /// forwarded verbatim — see `ClientMessage::RelayLanPacket`.
    LanPacket {
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
    },
    /// A chat message broadcast to every client connected to this server —
    /// one global channel per server, not per room. `unix_millis` is a UTC
    /// epoch timestamp; the client formats it in whatever timezone that
    /// machine is actually in, never UTC-for-everyone.
    ChatMessage {
        from_display_name: String,
        from_client_id: Uuid,
        text: String,
        unix_millis: u64,
    },
    /// A private message from another user.
    PrivateMessage {
        from_client_id: Uuid,
        to_client_id: Uuid,
        from_display_name: String,
        text: String,
        unix_millis: u64,
    },
    Error {
        message: String,
    },
    Pong,
}

impl ClientMessage {
    pub fn to_json(&self) -> Result<String, ProtocolError> {
        serde_json::to_string(self).map_err(ProtocolError::JsonSerialize)
    }

    pub fn from_json(text: &str) -> Result<Self, ProtocolError> {
        serde_json::from_str(text).map_err(ProtocolError::JsonDeserialize)
    }
}

impl ServerMessage {
    pub fn to_json(&self) -> Result<String, ProtocolError> {
        serde_json::to_string(self).map_err(ProtocolError::JsonSerialize)
    }

    pub fn from_json(text: &str) -> Result<Self, ProtocolError> {
        serde_json::from_str(text).map_err(ProtocolError::JsonDeserialize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_roundtrips_through_json() {
        let message = ClientMessage::CreateRoom {
            name: "Torneo de clanes".to_string(),
            game_port: 6112,
            password: Some("hunter2".to_string()),
        };
        let json = message.to_json().expect("encode should succeed");
        assert!(json.contains("\"type\":\"createRoom\""));
        assert!(json.contains("\"gamePort\":6112"));

        let decoded = ClientMessage::from_json(&json).expect("decode should succeed");
        match decoded {
            ClientMessage::CreateRoom {
                name,
                game_port,
                password,
            } => {
                assert_eq!(name, "Torneo de clanes");
                assert_eq!(game_port, 6112);
                assert_eq!(password.as_deref(), Some("hunter2"));
            }
            other => panic!("unexpected variant decoded: {other:?}"),
        }
    }

    #[test]
    fn server_message_roundtrips_through_json() {
        let room = RoomSummary {
            id: Uuid::new_v4(),
            name: "Sala de prueba".to_string(),
            host_display_name: "Thrall".to_string(),
            host_client_id: Uuid::new_v4(),
            game_port: 6112,
            status: RoomStatus::Available,
            player_count: 1,
            created_unix_secs: 1_700_000_000,
            has_password: true,
        };
        let message = ServerMessage::RoomCreated {
            room: room.clone(),
            relay_token: room.id,
        };

        let json = message.to_json().expect("encode should succeed");
        let decoded = ServerMessage::from_json(&json).expect("decode should succeed");
        match decoded {
            ServerMessage::RoomCreated {
                room: decoded_room,
                relay_token,
            } => {
                assert_eq!(decoded_room.id, room.id);
                assert_eq!(relay_token, room.id);
            }
            other => panic!("unexpected variant decoded: {other:?}"),
        }
    }

    #[test]
    fn lan_packet_bytes_roundtrip_as_compact_base64_not_a_json_number_array() {
        let raw = vec![0xF7_u8, 0x30, 0x10, 0x00, 1, 2, 3, 4, 0, 255];
        let message = ClientMessage::RelayLanPacket { bytes: raw.clone() };

        let json = message.to_json().expect("encode should succeed");
        assert!(
            !json.contains('['),
            "bytes should be base64, not a JSON number array: {json}"
        );

        let decoded = ClientMessage::from_json(&json).expect("decode should succeed");
        match decoded {
            ClientMessage::RelayLanPacket { bytes } => assert_eq!(bytes, raw),
            other => panic!("unexpected variant decoded: {other:?}"),
        }
    }
}
