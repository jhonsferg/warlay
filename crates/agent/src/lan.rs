use std::sync::Arc;

use tracing::info;

use crate::state::DaemonState;

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

    let loop_guard_for_capture = s.lan_loop_guard.clone();
    let game_started_for_capture = s.game_started.clone();
    let own_game_hosting_for_capture = s.own_game_hosting.clone();

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
            // Echo check FIRST: every packet we replay onto the local
            // network comes back through this promiscuous socket as an
            // echo of our own IP, and now that every member's captures are
            // relayed here too, letting echoes through would both re-relay
            // them and — worse — misread another member's re-announced game
            // as OUR OWN, firing a bogus GameEnded on whoever happened to
            // be hosting. Everything below therefore reads only genuinely
            // own-WC3 traffic.
            //
            // Observer-only, same contract as the echo check: bytes are
            // only read here, never modified/delayed, still forwarded as-is
            // via `cap_tx.send(bytes)`.
            if loop_guard_for_capture.is_echo(&bytes) {
                return;
            }
            // Host-state sniff for `own_game_hosting`: a WC3 screen that
            // announces (CREATEGAME/REFRESHGAME — both observed coming from
            // a hosting session) is hosting; one that searches
            // (SEARCHGAME) or destroys its game (DECREATEGAME) is browsing
            // again. A re-announced game is also the only real signal W3GS
            // gives us that a previous session is over (see
            // `contains_create_game`'s doc) -- the swap only fires once per
            // detected game (and never on the room's very first
            // CREATEGAME, since `game_started` starts false).
            if warlay::engine::contains_create_game(&bytes) || warlay::engine::contains_refresh_game(&bytes) {
                own_game_hosting_for_capture.store(true, std::sync::atomic::Ordering::SeqCst);
                if game_started_for_capture.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    let _ = game_ended_tx.send(());
                }
            } else if warlay::engine::contains_search_game(&bytes) || warlay::engine::contains_decreate_game(&bytes) {
                own_game_hosting_for_capture.store(false, std::sync::atomic::Ordering::SeqCst);
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
                crate::ipc_server::push_event(&state_for_game_ended, warlay::protocol::ipc::DaemonResponse::GameEnded)
                    .await;
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
