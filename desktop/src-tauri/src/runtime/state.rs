use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use tracing::info;

use warlay::engine::{CapturePorts, LoopGuard};

pub struct RuntimeState {
    pub game_tunnel: Option<warlay::engine::TunnelHandle>,
    pub peer_mesh: Option<Arc<warlay::engine::PeerMeshHandle>>,
    /// The active room session (new architecture: helper + ticket), if any.
    pub room_session: Option<super::session::RoomSessionInfo>,
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
    /// Set by the `COUNTDOWN_START` sniff (see `StartHostingTunnel`'s
    /// `on_game_started` callback), cleared by the `CREATEGAME` sniff (see
    /// `spawn_lan_mesh`'s capture callback) -- an atomic check-and-reset so
    /// a re-announced game only flips the room back to `Available` once,
    /// and never on the room's very first `CREATEGAME` (the flag starts
    /// `false`, so that one is a no-op).
    pub game_started: Arc<std::sync::atomic::AtomicBool>,
    /// Switch the LAN-capture callback (`lan.rs`) checks on every captured
    /// packet. Always on: LAN discovery is part of how Warlay works.
    pub lan_discovery_enabled: Arc<AtomicBool>,
}

/// Aborts whatever tunnel/LAN-capture/rebroadcast tasks are currently
/// running for the active session. Shared by every request that tears one
/// down (`DisconnectFromHub`, `LeaveRoom`, `DeleteRoom`, `StopHubTunnel`,
/// and now graceful shutdown) -- previously copy-pasted four times.
impl RuntimeState {
    /// This machine's LAN-bridging rules over the shared state (see
    /// `warlay::engine::LanBridge`). Cheap: clones a few `Arc`s.
    pub fn lan_bridge(&self) -> warlay::engine::LanBridge {
        warlay::engine::LanBridge {
            loop_guard: self.lan_loop_guard.clone(),
            own_game_hosting: self.own_game_hosting.clone(),
            game_started: self.game_started.clone(),
            capture_ports: self.lan_capture_ports.clone(),
        }
    }

    pub fn new() -> Self {
        Self {
            game_tunnel: None,
            peer_mesh: None,
            room_session: None,
            lan_capture: None,
            lan_rebroadcast: None,
            lan_relay_tx: None,
            lan_loop_guard: Arc::new(LoopGuard::new(Duration::from_secs(2))),
            lan_capture_ports: CapturePorts::default(),
            own_game_hosting: Arc::new(AtomicBool::new(false)),
            game_gateway: None,
            game_started: Arc::new(AtomicBool::new(false)),
            lan_discovery_enabled: Arc::new(AtomicBool::new(true)),
        }
    }
}

pub fn teardown_tunnel_and_lan(s: &mut RuntimeState) {
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

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
