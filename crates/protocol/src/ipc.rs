use crate::ServerMessage;
use serde::{Deserialize, Serialize};

/// A request from the UI client to the daemon, tagged with a correlation id
/// so the client can match the eventual reply to the call that sent it. The
/// daemon echoes `id` back as `DaemonResponseEnvelope::reply_to`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DaemonRequestEnvelope {
    pub id: u64,
    #[serde(flatten)]
    pub request: DaemonRequest,
}

/// A response or event from the daemon. `reply_to` is `Some(id)` when this
/// is the direct answer to a specific `DaemonRequestEnvelope`; `None` for
/// server-initiated state broadcasts (`HubMessage`, `HubPing`,
/// `TunnelStatusChanged`, ...) that aren't a reply to anything in
/// particular.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DaemonResponseEnvelope {
    pub reply_to: Option<u64>,
    #[serde(flatten)]
    pub response: DaemonResponse,
}

/// Commands sent from the UI Client to the Daemon.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "payload")]
pub enum DaemonRequest {
    Ping,

    ConnectToHub {
        server_id: String,
        address: String,
        psk: Option<[u8; 32]>,
        password: Option<String>,
        display_name: String,
        /// The desktop app's own persisted identity (`ClientStore::client_id`)
        /// — sent explicitly rather than letting the daemon invent its own,
        /// since `psk` is issued by the hub keyed to *this exact* client_id;
        /// a mismatched id makes an otherwise-valid psk look unrecognized to
        /// the hub (`AuthResponse::Rejected`) on every single reconnect.
        client_id: String,
    },

    DisconnectFromHub {
        server_id: String,
    },

    /// Disconnects from every hub the daemon currently holds a connection
    /// to, and tears down any active tunnel/LAN mesh. Used by the desktop
    /// app's tray "Salir" (a real quit, unlike the window's own close
    /// button, which just hides it) -- the daemon is a separate,
    /// independently-running process, so closing the desktop app's window
    /// or even exiting its process does *not* by itself close the daemon's
    /// own hub WebSocket connections; without this, other clients keep
    /// seeing this user as present/in their room indefinitely.
    DisconnectAll,

    /// Asks the daemon to shut itself down: it drops every hub connection,
    /// leaves the room's mesh, tears down the tunnel and the LAN capture,
    /// answers, and then exits its process. The desktop app's tray "Salir"
    /// sends it: the daemon runs elevated, so the (non-elevated) app can't
    /// terminate it from outside -- it has to be asked to stop itself.
    Shutdown,

    CreateRoom {
        server_id: String,
        name: String,
        game_port: u16,
        password: Option<String>,
    },

    JoinRoom {
        server_id: String,
        room_id: String,
        password: Option<String>,
    },

    LeaveRoom {
        server_id: String,
    },

    DeleteRoom {
        server_id: String,
        room_id: String,
    },

    SetRoomInProgress {
        server_id: String,
    },

    SetRoomAvailable {
        server_id: String,
    },

    SendChatMessage {
        server_id: String,
        text: String,
    },

    SendPrivateMessage {
        server_id: String,
        to_client_id: String,
        text: String,
    },

    /// `secure` wraps the tunnel in TLS, verifying the hub's certificate
    /// against `hub_host` -- set when the server was registered as `wss://`.
    /// Defaults to `false` so an older desktop app keeps talking plain TCP.
    StartHostingTunnel {
        hub_host: String,
        hub_tcp_port: u16,
        room_token: String,
        game_port: u16,
        #[serde(default)]
        secure: bool,
    },

    StartJoiningTunnel {
        hub_host: String,
        hub_tcp_port: u16,
        room_token: String,
        game_port: u16,
        #[serde(default)]
        secure: bool,
    },

    StopHubTunnel,

    LaunchGame {
        path: String,
        args: Vec<String>,
    },

    VoiceFrameOut {
        room_token: String,
        opus_bytes: Vec<u8>,
    },

    /// Reads the daemon's own persisted configuration (log level, LAN
    /// discovery toggle, ...) — separate from `AppSettings`, which is the
    /// desktop client's own settings and lives in a different table.
    GetAgentSettings,

    /// Applies `settings` to the running daemon immediately (log level
    /// reload, LAN discovery toggle) and persists it.
    SetAgentSettings {
        settings: AgentSettings,
    },

    /// Snapshot of what the daemon is doing right now — hub connections,
    /// tunnel/mesh state, per-peer P2P link state — for the "Agente" panel's
    /// diagnostic view.
    GetAgentStatus,

    /// Resets any peer currently stuck in `PeerLinkState::Failed` back to
    /// `NotAttempted` so the mesh's own retry loop (already running every
    /// 3s) picks it up again, without leaving/rejoining the room.
    RetryPeerMesh,

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
    StartGameTunnel {
        game_port: u16,
    },

    /// The room moved to another helper (the old one died): keep the mesh
    /// and its links, just register and relay through the new one.
    UpdateRoomHelper {
        rendezvous_addr: String,
        relay_addr: String,
    },
}

impl DaemonRequest {
    /// The variant's name only, never its payload — safe to log even for
    /// requests carrying secrets (`ConnectToHub`'s `psk`/`password`).
    pub fn variant_name(&self) -> &'static str {
        match self {
            DaemonRequest::Ping => "Ping",
            DaemonRequest::ConnectToHub { .. } => "ConnectToHub",
            DaemonRequest::DisconnectFromHub { .. } => "DisconnectFromHub",
            DaemonRequest::DisconnectAll => "DisconnectAll",
            DaemonRequest::Shutdown => "Shutdown",
            DaemonRequest::CreateRoom { .. } => "CreateRoom",
            DaemonRequest::JoinRoom { .. } => "JoinRoom",
            DaemonRequest::LeaveRoom { .. } => "LeaveRoom",
            DaemonRequest::DeleteRoom { .. } => "DeleteRoom",
            DaemonRequest::SetRoomInProgress { .. } => "SetRoomInProgress",
            DaemonRequest::SetRoomAvailable { .. } => "SetRoomAvailable",
            DaemonRequest::SendChatMessage { .. } => "SendChatMessage",
            DaemonRequest::SendPrivateMessage { .. } => "SendPrivateMessage",
            DaemonRequest::StartHostingTunnel { .. } => "StartHostingTunnel",
            DaemonRequest::StartJoiningTunnel { .. } => "StartJoiningTunnel",
            DaemonRequest::StopHubTunnel => "StopHubTunnel",
            DaemonRequest::LaunchGame { .. } => "LaunchGame",
            DaemonRequest::VoiceFrameOut { .. } => "VoiceFrameOut",
            DaemonRequest::GetAgentSettings => "GetAgentSettings",
            DaemonRequest::SetAgentSettings { .. } => "SetAgentSettings",
            DaemonRequest::GetAgentStatus => "GetAgentStatus",
            DaemonRequest::RetryPeerMesh => "RetryPeerMesh",
            DaemonRequest::StartRoomSession { .. } => "StartRoomSession",
            DaemonRequest::StopRoomSession => "StopRoomSession",
            DaemonRequest::StartGameTunnel { .. } => "StartGameTunnel",
            DaemonRequest::UpdateRoomHelper { .. } => "UpdateRoomHelper",
        }
    }
}

/// The daemon's own persisted configuration — distinct from the desktop
/// client's `AppSettings` (different process, different SQLite table, see
/// `daemon/src/storage.rs`).
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettings {
    #[serde(default = "default_agent_log_level")]
    pub log_level: String,
    #[serde(default = "default_true")]
    pub lan_discovery_enabled: bool,
}

fn default_agent_log_level() -> String {
    "info".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            log_level: default_agent_log_level(),
            lan_discovery_enabled: default_true(),
        }
    }
}

/// One P2P mesh peer's direct-link negotiation state, for the "Agente"
/// diagnostic panel — mirrors `engine::p2p_mesh::PeerLinkState` but as a
/// plain string tag so this crate doesn't need to depend on `engine`.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PeerLinkStatus {
    pub client_id: String,
    pub is_host: bool,
    pub p2p_addr: Option<String>,
    /// `"not_attempted" | "connecting" | "connected" | "relayed" | "failed"`.
    pub state: String,
}

/// Reply to `GetAgentStatus` — a snapshot of what the daemon is doing right
/// now, for the "Agente" panel.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatusSnapshot {
    pub uptime_secs: u64,
    pub hub_connections: usize,
    pub tunnel_active: bool,
    pub lan_discovery_enabled: bool,
    pub mesh_peers: Vec<PeerLinkStatus>,
}

/// Why a `ConnectToHub` request failed, preserved structurally (rather than
/// collapsed into a string) so the UI can show a password prompt or a
/// rejection reason instead of a generic error.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "detail")]
pub enum ConnectFailure {
    PasswordRequired { server_name: String },
    Rejected { message: String, retry_as_new: bool },
    Other { message: String },
}

/// Responses and Events emitted from the Daemon to the UI Client.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "payload")]
pub enum DaemonResponse {
    Pong,

    Success {
        request_type: String,
    },

    Error {
        request_type: String,
        message: String,
    },

    /// Direct reply to a successful `ConnectToHub`, carrying what the UI
    /// needs to persist the handshake result locally (the server's own
    /// self-reported id/name and, on first contact or password auth, the
    /// freshly issued PSK).
    Connected {
        server_id: String,
        remote_server_id: String,
        server_name: String,
        new_psk: Option<[u8; 32]>,
    },

    /// Direct reply to a failed `ConnectToHub`.
    ConnectFailed {
        server_id: String,
        failure: ConnectFailure,
    },

    // --- State Broadcasts (Events) ---
    HubConnectionState {
        server_id: String,
        connected: bool,
    },

    HubPing {
        server_id: String,
        latency_ms: u32,
    },

    HubMessage {
        server_id: String,
        message: ServerMessage,
    },

    TunnelStatusChanged {
        running: bool,
    },

    VoiceFrameIn {
        from_client_id: String,
        opus_bytes: Vec<u8>,
    },

    /// Direct reply to `GetAgentSettings`.
    AgentSettings {
        settings: AgentSettings,
    },

    /// Direct reply to `GetAgentStatus`.
    AgentStatus {
        status: AgentStatusSnapshot,
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
    fn shutdown_travels_as_a_bare_tagged_request() {
        let json = serde_json::to_string(&DaemonRequest::Shutdown).unwrap();
        assert_eq!(json, r#"{"type":"Shutdown"}"#);
        let back: DaemonRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, DaemonRequest::Shutdown));
        assert_eq!(back.variant_name(), "Shutdown");
    }

    #[test]
    fn agent_settings_default_to_a_quiet_log_level() {
        let settings: AgentSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.log_level, "info");
        assert!(settings.lan_discovery_enabled);
    }
}
