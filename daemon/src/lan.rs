use std::sync::Arc;

use tracing::info;

use crate::state::DaemonState;

pub async fn spawn_lan_mesh(
    state_ref: Arc<tokio::sync::Mutex<DaemonState>>,
    game_port: u16,
    join_port: Option<u16>,
) -> Result<(), String> {
    let mut s = state_ref.lock().await;
    if let Some(c) = s.lan_capture.take() {
        c.abort();
    }
    if let Some(r) = s.lan_rebroadcast.take() {
        r.abort();
    }

    let capture_port = join_port.unwrap_or(game_port);

    let (tx_rebroadcast, rx_rebroadcast) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    s.lan_relay_tx = Some(tx_rebroadcast);

    let loop_guard_for_capture = s.lan_loop_guard.clone();
    let game_started_for_capture = s.game_started.clone();

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
        warlay::engine::spawn_lan_capture(capture_port, lan_discovery_enabled, move |bytes| {
            // Observer-only, same as the echo check below: bytes are only
            // read here, never modified/delayed, still forwarded as-is via
            // `cap_tx.send(bytes)`. A re-announced game is the only real
            // signal W3GS gives us that a previous session is over (see
            // `contains_create_game`'s doc comment) -- the swap only fires
            // once per detected game (and never on the room's very first
            // CREATEGAME, since the flag starts false).
            if warlay::engine::contains_create_game(&bytes)
                && game_started_for_capture.swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                let _ = game_ended_tx.send(());
            }
            if !loop_guard_for_capture.is_echo(&bytes) {
                let _ = cap_tx.send(bytes);
            }
        })
        .map_err(|e| format!("no se pudo iniciar la captura LAN: {e}"))?,
    );

    let state_for_game_ended = state_ref.clone();
    tokio::spawn(async move {
        while game_ended_rx.recv().await.is_some() {
            let s = state_for_game_ended.lock().await;
            if let Some(active_id) = &s.active_room_server {
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
            join_port, // override_port: redirect joiners' GAMEINFO to our tunnel's real port
            move |bytes| {
                let _ = cap_tx_clone.send(bytes);
            },
        )
        .await
        .map_err(|e| format!("no se pudo iniciar la retransmisión LAN: {e}"))?,
    );

    let active_id = s.active_room_server.clone();
    let state_for_task = state_ref.clone();

    info!(game_port, capture_port, ?join_port, "LAN mesh started");

    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(payload) = cap_rx.recv() => {
                    let s_guard = state_for_task.lock().await;
                    if let Some(active) = &active_id {
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
