use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tracing::info;

use warlay::engine::MatchmakingConnection;
use warlay::protocol::ipc::DaemonResponseEnvelope;

use crate::storage::AgentStore;

pub struct DaemonState {
    pub connections: HashMap<String, MatchmakingConnection>,
    pub game_tunnel: Option<warlay::engine::TunnelHandle>,
    pub peer_mesh: Option<Arc<warlay::engine::PeerMeshHandle>>,
    /// The active room session (new architecture: helper + ticket), if any.
    pub room_session: Option<crate::session::RoomSessionInfo>,
    pub voice_poll_task: Option<tokio::task::JoinHandle<()>>,
    pub lan_capture: Option<warlay::engine::LanCaptureHandle>,
    pub lan_rebroadcast: Option<warlay::engine::LanRebroadcastHandle>,
    pub lan_relay_tx: Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>,
    pub lan_loop_guard: Arc<warlay::engine::LoopGuard>,
    /// The UDP ports the LAN capture currently relays: the game port plus
    /// every per-peer joiner port (`GameGatewayHandle::ensure_listener`)
    /// appended as those peers' rewritten `GAMEINFO`s get replayed locally
    /// -- shared live with the capture's raw threads, see
    /// `lan_filter::CapturePorts`.
    pub lan_capture_ports: warlay::engine::CapturePorts,
    /// True while THIS machine's own WC3 is hosting a game: from its own
    /// `CREATEGAME`/`REFRESHGAME` broadcast until its next
    /// `SEARCHGAME`/`DECREATEGAME` (sniffed in `lan.rs`'s capture
    /// callback). Gates which remote packets are replayed onto the local
    /// network (`session.rs` and `handlers.rs`): while we host, remote
    /// game state is both useless to us (we're in our own lobby) and the
    /// one thing that must not reach a WC3 mid-create.
    pub own_game_hosting: Arc<AtomicBool>,
    /// Per-peer local listeners that carry connections to other members'
    /// games over the mesh, while a room's game tunnel is up (see
    /// `start_game_gateway`): one dedicated port per peer, so the port in
    /// a rewritten `GAMEINFO` says whose game the local WC3 is joining.
    pub game_gateway: Option<Arc<warlay::engine::GameGatewayHandle>>,
    pub active_room_server: Option<String>,
    /// Which specific room `active_room_server` refers to. Needed because
    /// every hub-pushed `ServerMessage` (`RoomCreated`, `RoomRemoved`, ...)
    /// is handled in its own independently-spawned task with no ordering
    /// guarantee between them: creating a new room while already hosting
    /// one makes the hub auto-delete the old room first (broadcasting
    /// `RoomRemoved` for it) and then send `RoomCreated` for the new one --
    /// if the `RoomRemoved` handler task for the stale room happens to
    /// finish *after* the `RoomCreated` handler for the new room already
    /// set up its mesh, a check keyed only on "is there *some* active room
    /// on this server" can't tell the two apart and wrongly tears down the
    /// brand new room's state. Comparing this against the room ID actually
    /// named in the message closes that race.
    pub active_room_id: Option<uuid::Uuid>,
    /// The pipe connection currently considered "the UI" for the purposes
    /// of push events -- updated on every new connection so a UI restart
    /// doesn't leave old callbacks writing into a dead channel.
    pub current_tx: Option<tokio::sync::mpsc::Sender<DaemonResponseEnvelope>>,
    /// Set by the `COUNTDOWN_START` sniff (see `StartHostingTunnel`'s
    /// `on_game_started` callback), cleared by the `CREATEGAME` sniff (see
    /// `spawn_lan_mesh`'s capture callback) -- an atomic check-and-reset so
    /// a re-announced game only flips the room back to `Available` once,
    /// and never on the room's very first `CREATEGAME` (the flag starts
    /// `false`, so that one is a no-op).
    pub game_started: Arc<std::sync::atomic::AtomicBool>,
    /// The daemon's own settings store -- see `storage.rs`'s doc comment
    /// for why this is a separate table from the desktop client's own
    /// store, even though both share the same `warlay.db` file.
    pub agent_store: AgentStore,
    /// Live toggle checked by the LAN-capture callback (`lan.rs`) on every
    /// captured packet -- an `Arc<AtomicBool>` rather than a value read
    /// once at spawn time, so flipping it from the "Agente" panel takes
    /// effect immediately even for a capture that's already running, no
    /// tunnel restart needed.
    pub lan_discovery_enabled: Arc<AtomicBool>,
    /// For the "Agente" panel's uptime display.
    pub started_at: std::time::Instant,
}

/// Aborts whatever tunnel/LAN-capture/rebroadcast tasks are currently
/// running for the active session. Shared by every request that tears one
/// down (`DisconnectFromHub`, `LeaveRoom`, `DeleteRoom`, `StopHubTunnel`,
/// and now graceful shutdown) -- previously copy-pasted four times.
pub fn teardown_tunnel_and_lan(s: &mut DaemonState) {
    let had_tunnel = s.game_tunnel.is_some();
    let had_capture = s.lan_capture.is_some();
    let had_rebroadcast = s.lan_rebroadcast.is_some();
    let had_gateway = s.game_gateway.is_some();

    if let Some(t) = s.game_tunnel.take() {
        t.abort();
    }
    if let Some(g) = s.game_gateway.take() {
        g.abort();
    }
    if let Some(c) = s.lan_capture.take() {
        c.abort();
    }
    if let Some(r) = s.lan_rebroadcast.take() {
        r.abort();
    }
    s.lan_relay_tx = None;

    if had_tunnel || had_capture || had_rebroadcast || had_gateway {
        info!(
            had_tunnel,
            had_capture, had_rebroadcast, had_gateway, "tunnel/LAN mesh torn down"
        );
    }
}
