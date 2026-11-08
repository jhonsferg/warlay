use std::sync::Arc;

use tracing::info;

use super::state::DaemonState;
use warlay::protocol::ipc::DaemonResponse;

/// Starts this machine's LAN side of the room: the scoped capture, the
/// rebroadcast socket, and the outbound relay to the other members.
///
/// `override_port`: legacy hub sessions redirect the `GAMEINFO` they
/// replay onto the local network to one fixed tunnel port. Room sessions
/// pass `None` — there, each peer's `GAMEINFO` is rewritten to that peer's
/// own dedicated joiner port by `session.rs` *before* the rebroadcast.
pub async fn spawn_lan_mesh(
    state_ref: Arc<tokio::sync::Mutex<DaemonState>>,
    game_port: u16,
    override_port: Option<u16>,
) -> Result<(), String> {
    let mut s = state_ref.lock().await;
    if let Some(c) = s.lan_capture.take() {
        c.abort();
    }
    if let Some(r) = s.lan_rebroadcast.take() {
        r.abort();
    }

    // The room's game port (and, in legacy sessions, the fixed tunnel
    // port) are the capture's starting set. Every member starts with the
    // game port — not just the room host: anyone may host a game the
    // others join, and a browsing WC3's own queries toward the games it
    // sees must be relayed on every member, not only one. Room sessions
    // append each peer's joiner port as its rewritten GAMEINFO gets
    // replayed locally, so the scope grows with the games this WC3 can
    // actually see and join.
    let capture_ports: warlay::engine::CapturePorts = Arc::new(std::sync::Mutex::new(match override_port {
        Some(port) if port != game_port => vec![game_port, port],
        _ => vec![game_port],
    }));
    s.lan_capture_ports = capture_ports.clone();
    // A fresh tunnel starts with nothing of ours hosting: the sniff in the
    // capture callback re-sets it the moment our own WC3 announces a game.
    s.own_game_hosting.store(false, std::sync::atomic::Ordering::Relaxed);

    let (tx_rebroadcast, rx_rebroadcast) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    s.lan_relay_tx = Some(tx_rebroadcast);

    let bridge_for_capture = s.lan_bridge();

    let (cap_tx, mut cap_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let cap_tx_clone = cap_tx.clone();

    // `spawn_lan_capture`'s callback runs on a raw `std::thread` (see
    // `lan_capture.rs`'s `run_capture_loop`), never a tokio task -- calling
    // `tokio::spawn` directly from it panics ("no reactor running") since
    // that thread has no tokio runtime context at all. An unbounded mpsc
    // `send` is safe from any thread, sync or async, so that's the only
    // thing this callback is allowed to do; the actual state-lock/hub-send
    // work happens in the task below, which genuinely runs on the runtime
    // (this whole function is `async`, spawned from `handle_request`).
    let (game_ended_tx, mut game_ended_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    let lan_discovery_enabled = s.lan_discovery_enabled.clone();

    s.lan_capture = Some(
        warlay::engine::spawn_lan_capture(capture_ports, lan_discovery_enabled, move |bytes| {
            // Echo check, host/browse state tracking and game-ended
            // detection are `LanBridge::classify_own_capture`; this callback
            // runs on a raw thread so it may only send on the channel.
            match bridge_for_capture.classify_own_capture(&bytes) {
                warlay::engine::OwnCapture::Echo => return,
                warlay::engine::OwnCapture::Forward { game_ended } => {
                    if game_ended {
                        let _ = game_ended_tx.send(());
                    }
                }
            }
            let _ = cap_tx.send(bytes);
        })
        .map_err(|e| format!("no se pudo iniciar la captura LAN: {e}"))?,
    );

    let state_for_game_ended = state_ref.clone();
    tokio::spawn(async move {
        while game_ended_rx.recv().await.is_some() {
            let s = state_for_game_ended.lock().await;
            if s.room_session.is_some() {
                // The UI owns the room on the main server.
                drop(s);
                info!("CREATEGAME detectado: partida anterior terminó");
                super::events::emit_event(&DaemonResponse::GameEnded)
            } else if let Some(active_id) = &s.active_room_server {
                if let Some(conn) = s.connections.get(active_id) {
                    info!(server_id = %active_id, "CREATEGAME detectado: partida anterior terminó, marcando sala disponible");
                    conn.send(warlay::protocol::ClientMessage::SetRoomAvailable);
                }
            } else {
                info!("CREATEGAME detectado pero no hay sala activa registrada, se ignora");
            }
        }
    });

    s.lan_rebroadcast = Some(
        warlay::engine::spawn_lan_rebroadcast(
            game_port,
            rx_rebroadcast,
            override_port, // legacy fixed redirect; room sessions rewrite per peer upstream
            move |bytes| {
                let _ = cap_tx_clone.send(bytes);
            },
        )
        .await
        .map_err(|e| format!("no se pudo iniciar la retransmisión LAN: {e}"))?,
    );

    let active_id = s.active_room_server.clone();
    let state_for_task = state_ref.clone();

    info!(game_port, ?override_port, "LAN mesh started");

    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(payload) = cap_rx.recv() => {
                    let s_guard = state_for_task.lock().await;
                    // Room sessions carry LAN broadcasts straight to the
                    // other players over the mesh, never through a server.
                    if s_guard.room_session.is_some() {
                        if let Some(mesh) = &s_guard.peer_mesh {
                            mesh.broadcast_lan_packet(&payload);
                        }
                    } else if let Some(active) = &active_id {
                        if let Some(conn) = s_guard.connections.get(active) {
                            conn.send(warlay::protocol::ClientMessage::RelayLanPacket { bytes: payload });
                        }
                    }
                }
                else => break,
            }
        }
    });

    Ok(())
}
