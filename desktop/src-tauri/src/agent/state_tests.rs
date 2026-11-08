use super::*;
use std::sync::atomic::Ordering;

#[test]
fn a_fresh_state_is_idle() {
    let s = AgentState::new();
    assert!(s.connections.is_empty());
    assert!(s.room_session.is_none());
    assert!(s.peer_mesh.is_none());
    assert!(s.game_gateway.is_none());
    assert!(!s.own_game_hosting.load(Ordering::SeqCst));
    assert!(!s.game_started.load(Ordering::SeqCst));
}

#[test]
fn the_lan_bridge_shares_live_state_with_the_agent() {
    let s = AgentState::new();
    let bridge = s.lan_bridge();

    // Flags flipped through the bridge (the capture thread) are what the
    // agent reads, and the reverse.
    bridge.own_game_hosting.store(true, Ordering::SeqCst);
    assert!(s.own_game_hosting.load(Ordering::SeqCst));
    s.game_started.store(true, Ordering::SeqCst);
    assert!(bridge.game_started.load(Ordering::SeqCst));

    // A port scoped through the bridge widens the capture's port set.
    bridge.capture_ports.lock().unwrap().push(40_000);
    assert!(s.lan_capture_ports.lock().unwrap().contains(&40_000));

    // One loop guard: a packet marked via the bridge is an echo for all.
    bridge.loop_guard.mark(b"packet");
    assert!(s.lan_loop_guard.is_echo(b"packet"));
}
