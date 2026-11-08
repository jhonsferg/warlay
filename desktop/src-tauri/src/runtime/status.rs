//! The network status, read in-process and pushed to the UI.
//!
//! A single watcher task looks at the runtime state once a second and emits
//! the [`STATUS_EVENT`] Tauri event only when something changed (the game
//! tunnel, P2P links or a real shift in their ping). Nothing is
//! sent while nothing happens; the UI still reads the current value once on
//! demand through `get_network_status`.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use warlay::protocol::ipc::{NetworkStatus, PeerLinkStatus};

use super::state::RuntimeState;

/// Tauri event carrying a [`NetworkStatus`].
pub const STATUS_EVENT: &str = "network://status";

/// How often the watcher looks at the state. Cheap: one lock and a copy.
const WATCH_INTERVAL: Duration = Duration::from_secs(1);

/// What the network is doing right now.
pub fn snapshot(s: &RuntimeState) -> NetworkStatus {
    let mesh_peers = s
        .peer_mesh
        .as_ref()
        .map(|mesh| {
            mesh.peer_statuses()
                .into_iter()
                .map(|(client_id, is_host, p2p_addr, state, rtt_ms)| PeerLinkStatus {
                    client_id: client_id.to_string(),
                    is_host,
                    p2p_addr: p2p_addr.map(|a| a.to_string()),
                    state: state.to_string(),
                    rtt_ms,
                })
                .collect()
        })
        .unwrap_or_default();

    NetworkStatus {
        tunnel_active: s.game_tunnel.is_some(),
        mesh_peers,
    }
}

/// Whether `next` is worth telling the UI about: the very first status, or
/// any change other than uptime.
pub fn should_emit(last: Option<&NetworkStatus>, next: &NetworkStatus) -> bool {
    last.map_or(true, |last| !last.same_state_as(next))
}

/// Starts the watcher that emits [`STATUS_EVENT`] on every real change.
pub fn spawn_watcher(app: AppHandle, state: Arc<Mutex<RuntimeState>>) {
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

    fn linked(rtt_ms: u32) -> PeerLinkStatus {
        PeerLinkStatus {
            client_id: "a".to_string(),
            is_host: false,
            p2p_addr: None,
            state: "connected".to_string(),
            rtt_ms: Some(rtt_ms),
        }
    }

    #[test]
    fn a_fresh_runtime_reports_an_idle_network() {
        let status = snapshot(&RuntimeState::new());
        assert!(!status.tunnel_active);
        assert!(status.mesh_peers.is_empty());
    }

    #[test]
    fn the_first_status_is_always_emitted() {
        assert!(should_emit(None, &snapshot(&RuntimeState::new())));
    }

    #[test]
    fn an_unchanged_status_is_not_emitted_again() {
        let s = RuntimeState::new();
        let first = snapshot(&s);
        assert!(!should_emit(Some(&first), &snapshot(&s)));
    }

    #[test]
    fn a_real_change_is_emitted() {
        let before = snapshot(&RuntimeState::new());
        let mut after = before.clone();
        after.tunnel_active = true;
        assert!(should_emit(Some(&before), &after));
    }

    #[test]
    fn ping_jitter_is_not_emitted_but_a_real_shift_is() {
        let mut before = snapshot(&RuntimeState::new());
        before.mesh_peers = vec![linked(40)];
        let mut jitter = before.clone();
        jitter.mesh_peers = vec![linked(46)];
        let mut shift = before.clone();
        shift.mesh_peers = vec![linked(95)];
        assert!(!should_emit(Some(&before), &jitter));
        assert!(should_emit(Some(&before), &shift));
    }
}
