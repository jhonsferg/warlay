//! The agent's network status, read in-process and pushed to the UI.
//!
//! The UI used to poll `get_network_status` every few seconds. Now a single
//! watcher task looks at the agent's state once a second and emits the
//! [`STATUS_EVENT`] Tauri event only when something other than uptime
//! changed (hub connections, tunnel, LAN discovery, P2P links). Nothing is
//! sent while nothing happens; the UI still reads the current value once on
//! demand through `get_network_status`.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use warlay::protocol::ipc::{NetworkStatus, PeerLinkStatus};

use super::state::AgentState;

/// Tauri event carrying a [`NetworkStatus`].
pub const STATUS_EVENT: &str = "network://status";

/// How often the watcher looks at the state. Cheap: one lock and a copy.
const WATCH_INTERVAL: Duration = Duration::from_secs(1);

/// What the agent is doing right now.
pub fn snapshot(s: &AgentState) -> NetworkStatus {
    let mesh_peers = s
        .peer_mesh
        .as_ref()
        .map(|mesh| {
            mesh.peer_statuses()
                .into_iter()
                .map(|(client_id, is_host, p2p_addr, state)| PeerLinkStatus {
                    client_id: client_id.to_string(),
                    is_host,
                    p2p_addr: p2p_addr.map(|a| a.to_string()),
                    state: state.to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    NetworkStatus {
        uptime_secs: s.started_at.elapsed().as_secs(),
        hub_connections: s.connections.len(),
        tunnel_active: s.game_tunnel.is_some(),
        lan_discovery_enabled: s.lan_discovery_enabled.load(std::sync::atomic::Ordering::Relaxed),
        mesh_peers,
    }
}

/// Whether `next` is worth telling the UI about: the very first status, or
/// any change other than uptime.
pub fn should_emit(last: Option<&NetworkStatus>, next: &NetworkStatus) -> bool {
    last.map_or(true, |last| !last.same_state_as(next))
}

/// Starts the watcher that emits [`STATUS_EVENT`] on every real change.
pub fn spawn_watcher(app: AppHandle, state: Arc<Mutex<AgentState>>) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<NetworkStatus> = None;
        let mut tick = tokio::time::interval(WATCH_INTERVAL);
        loop {
            tick.tick().await;
            let next = snapshot(&*state.lock().await);
            if should_emit(last.as_ref(), &next) {
                let _ = app.emit(STATUS_EVENT, &next);
                last = Some(next);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn a_fresh_agent_reports_an_idle_network() {
        let s = AgentState::new();
        let status = snapshot(&s);
        assert_eq!(status.hub_connections, 0);
        assert!(!status.tunnel_active);
        assert!(status.mesh_peers.is_empty());
        assert!(status.lan_discovery_enabled);
    }

    #[test]
    fn the_snapshot_follows_the_live_lan_discovery_toggle() {
        let s = AgentState::new();
        s.lan_discovery_enabled.store(false, Ordering::Relaxed);
        assert!(!snapshot(&s).lan_discovery_enabled);
        s.lan_discovery_enabled.store(true, Ordering::Relaxed);
        assert!(snapshot(&s).lan_discovery_enabled);
    }

    #[test]
    fn the_first_status_is_always_emitted() {
        assert!(should_emit(None, &snapshot(&AgentState::new())));
    }

    #[test]
    fn an_unchanged_status_is_not_emitted_again() {
        let s = AgentState::new();
        let first = snapshot(&s);
        let mut later = snapshot(&s);
        later.uptime_secs += 60;
        assert!(!should_emit(Some(&first), &later));
    }

    #[test]
    fn a_real_change_is_emitted() {
        let s = AgentState::new();
        let before = snapshot(&s);
        s.lan_discovery_enabled.store(false, Ordering::Relaxed);
        assert!(should_emit(Some(&before), &snapshot(&s)));
    }
}
