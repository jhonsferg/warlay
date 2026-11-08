use serde::{Deserialize, Serialize};

/// Commands the desktop client's Tauri commands hand to its network runtime
/// (room mesh and game tunnel). They never leave the process.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "payload")]
pub enum RuntimeRequest {
    /// Joins a room's P2P mesh through the helper the main server assigned
    /// (`rendezvous_addr`), proving membership with `ticket` (the base64
    /// room ticket from the main server's room session). Replaces any
    /// previous session. No game server is involved from here on.
    StartRoomSession {
        room_id: String,
        user_id: String,
        is_host: bool,
        rendezvous_addr: String,
        relay_addr: String,
        ticket: String,
    },

    /// Leaves the room's mesh and tears down its game tunnel.
    StopRoomSession,

    /// Starts carrying the game over the active room session: the host
    /// bridges peers to its local game, a joiner exposes a local port its
    /// game connects to; LAN announcements travel over the mesh.
    StartGameTunnel { game_port: u16 },

    /// The room moved to another helper (the old one died): keep the mesh
    /// and its links, just register and relay through the new one.
    UpdateRoomHelper {
        rendezvous_addr: String,
        relay_addr: String,
    },
}

impl RuntimeRequest {
    /// The variant's name only, never its payload — safe to log even for
    /// requests carrying secrets (the room `ticket`).
    pub fn variant_name(&self) -> &'static str {
        match self {
            RuntimeRequest::StartRoomSession { .. } => "StartRoomSession",
            RuntimeRequest::StopRoomSession => "StopRoomSession",
            RuntimeRequest::StartGameTunnel { .. } => "StartGameTunnel",
            RuntimeRequest::UpdateRoomHelper { .. } => "UpdateRoomHelper",
        }
    }
}

/// One P2P mesh peer's direct-link negotiation state — mirrors
/// `engine::p2p_mesh::PeerLinkState` but as a plain string tag so this crate
/// doesn't need to depend on `engine`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PeerLinkStatus {
    pub client_id: String,
    pub is_host: bool,
    pub p2p_addr: Option<String>,
    /// `"not_attempted" | "connecting" | "connected" | "relayed" | "failed"`.
    pub state: String,
    /// Round-trip time of the live link in milliseconds (the player's ping);
    /// `None` while there is no connection.
    #[serde(default)]
    pub rtt_ms: Option<u32>,
}

/// Ping changes smaller than this are noise, not news: the UI is not told
/// about them, so a link that wobbles by a few ms does not emit an event
/// every second.
const RTT_NOISE_MS: u32 = 10;

impl PeerLinkStatus {
    /// Same link, ignoring ping jitter below [`RTT_NOISE_MS`].
    fn same_link_as(&self, other: &PeerLinkStatus) -> bool {
        let rtt_close = match (self.rtt_ms, other.rtt_ms) {
            (Some(a), Some(b)) => a.abs_diff(b) < RTT_NOISE_MS,
            (None, None) => true,
            _ => false,
        };
        self.client_id == other.client_id
            && self.is_host == other.is_host
            && self.p2p_addr == other.p2p_addr
            && self.state == other.state
            && rtt_close
    }
}

/// A snapshot of what the network is doing right now: the game tunnel and
/// the P2P links. Pushed to the UI (as the `network://status` event)
/// whenever it changes.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatus {
    pub tunnel_active: bool,
    pub mesh_peers: Vec<PeerLinkStatus>,
}

impl NetworkStatus {
    /// Whether `other` describes the same network state, ignoring ping
    /// jitter under [`RTT_NOISE_MS`]. A status event is pushed to the UI
    /// only when this is false.
    pub fn same_state_as(&self, other: &NetworkStatus) -> bool {
        self.tunnel_active == other.tunnel_active
            && self.mesh_peers.len() == other.mesh_peers.len()
            && self
                .mesh_peers
                .iter()
                .zip(&other.mesh_peers)
                .all(|(a, b)| a.same_link_as(b))
    }
}

/// Replies and events the network runtime emits towards the UI.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "payload")]
pub enum RuntimeResponse {
    Success {
        request_type: String,
    },

    Error {
        request_type: String,
        message: String,
    },

    // --- State Broadcasts (Events) ---
    TunnelStatusChanged {
        running: bool,
    },

    /// The host's game started the match (`COUNTDOWN_START` seen): the UI
    /// marks the room in game on the main server.
    GameStarted,

    /// The host's game announced a new game after a match (`CREATEGAME`
    /// seen): the previous match is over.
    GameEnded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_request_travels_as_a_tagged_value() {
        let json = serde_json::to_string(&RuntimeRequest::StopRoomSession).unwrap();
        assert_eq!(json, r#"{"type":"StopRoomSession"}"#);
        let back: RuntimeRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, RuntimeRequest::StopRoomSession));
        assert_eq!(back.variant_name(), "StopRoomSession");
    }

    #[test]
    fn every_request_has_a_name_that_never_carries_its_payload() {
        let start = RuntimeRequest::StartRoomSession {
            room_id: "r".into(),
            user_id: "u".into(),
            is_host: true,
            rendezvous_addr: "1.2.3.4:5".into(),
            relay_addr: "1.2.3.4:6".into(),
            ticket: "secret-ticket".into(),
        };
        assert_eq!(start.variant_name(), "StartRoomSession");
        assert!(!start.variant_name().contains("secret"));
        assert_eq!(
            RuntimeRequest::StartGameTunnel { game_port: 6112 }.variant_name(),
            "StartGameTunnel"
        );
        assert_eq!(
            RuntimeRequest::UpdateRoomHelper {
                rendezvous_addr: String::new(),
                relay_addr: String::new()
            }
            .variant_name(),
            "UpdateRoomHelper"
        );
    }

    fn status(peers: Vec<PeerLinkStatus>) -> NetworkStatus {
        NetworkStatus {
            tunnel_active: false,
            mesh_peers: peers,
        }
    }

    fn peer(id: &str, state: &str) -> PeerLinkStatus {
        PeerLinkStatus {
            client_id: id.to_string(),
            is_host: false,
            p2p_addr: None,
            state: state.to_string(),
            rtt_ms: None,
        }
    }

    fn with_rtt(mut p: PeerLinkStatus, rtt_ms: u32) -> PeerLinkStatus {
        p.rtt_ms = Some(rtt_ms);
        p
    }

    #[test]
    fn small_ping_jitter_is_not_a_change_but_a_real_shift_is() {
        let a = status(vec![with_rtt(peer("a", "connected"), 40)]);
        let jitter = status(vec![with_rtt(peer("a", "connected"), 47)]);
        let shift = status(vec![with_rtt(peer("a", "connected"), 80)]);
        let lost = status(vec![peer("a", "connected")]);
        assert!(a.same_state_as(&jitter));
        assert!(!a.same_state_as(&shift));
        assert!(!a.same_state_as(&lost), "a link that loses its ping is news");
    }

    #[test]
    fn a_status_without_ping_still_deserializes() {
        let link: PeerLinkStatus =
            serde_json::from_str(r#"{"clientId":"a","isHost":false,"p2pAddr":null,"state":"connected"}"#).unwrap();
        assert_eq!(link.rtt_ms, None);
    }

    #[test]
    fn network_status_keeps_the_camel_case_shape_the_frontend_reads() {
        let json = serde_json::to_value(status(vec![peer("a", "connected")])).unwrap();
        for key in ["tunnelActive", "meshPeers"] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
        let link = &json["meshPeers"][0];
        for key in ["clientId", "isHost", "p2pAddr", "state", "rttMs"] {
            assert!(link.get(key).is_some(), "missing {key} in a link");
        }
    }

    #[test]
    fn an_identical_status_is_not_a_change() {
        let a = status(vec![peer("a", "connected")]);
        assert!(a.same_state_as(&a.clone()));
    }

    #[test]
    fn any_real_change_is_detected() {
        let base = status(vec![peer("a", "connecting")]);

        let mut tunnel = base.clone();
        tunnel.tunnel_active = true;
        let linked = status(vec![peer("a", "connected")]);
        let joined = status(vec![peer("a", "connecting"), peer("b", "connecting")]);

        for changed in [tunnel, linked, joined] {
            assert!(!base.same_state_as(&changed));
        }
    }
}
