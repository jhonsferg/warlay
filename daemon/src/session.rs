//! Room sessions of the new architecture: the main server hands the UI a
//! room ticket and the helper assigned to the room; the daemon joins the
//! room's P2P mesh through that helper and carries the game, its LAN
//! announcements and voice directly between the players.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, warn};
use uuid::Uuid;

use warlay::protocol::ipc::DaemonResponse;

use crate::ipc_server::{err, push_event};
use crate::lan::spawn_lan_mesh;
use crate::state::{teardown_tunnel_and_lan, DaemonState};

pub struct RoomSessionInfo {
    pub room_id: Uuid,
    pub is_host: bool,
    /// Replays LAN broadcasts received over the mesh onto this machine's LAN.
    lan_inbound_task: JoinHandle<()>,
}

impl Drop for RoomSessionInfo {
    fn drop(&mut self) {
        self.lan_inbound_task.abort();
    }
}

async fn resolve(addr: &str) -> Result<SocketAddr, String> {
    tokio::net::lookup_host(addr)
        .await
        .map_err(|e| format!("no se pudo resolver '{addr}': {e}"))?
        .next()
        .ok_or_else(|| format!("'{addr}' no resolvió a ninguna IP"))
}

/// Ends the current room session (mesh, game tunnel, LAN bridging).
pub fn clear_room_session(s: &mut DaemonState) {
    if let Some(mesh) = s.peer_mesh.take() {
        mesh.abort();
    }
    if let Some(task) = s.voice_poll_task.take() {
        task.abort();
    }
    s.room_session = None;
    teardown_tunnel_and_lan(s);
}

/// Forwards voice frames received over the mesh to the UI.
pub fn spawn_voice_poll(state: Arc<Mutex<DaemonState>>, mesh: Arc<warlay::engine::PeerMeshHandle>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(15));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            for (from_client_id, opus_bytes) in mesh.poll_datagrams() {
                push_event(
                    &state,
                    DaemonResponse::VoiceFrameIn {
                        from_client_id: from_client_id.to_string(),
                        opus_bytes,
                    },
                )
                .await;
            }
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn start_room_session(
    state: Arc<Mutex<DaemonState>>,
    room_id: String,
    user_id: String,
    is_host: bool,
    rendezvous_addr: String,
    relay_addr: String,
    ticket: String,
) -> DaemonResponse {
    let (Ok(room_id), Ok(user_id)) = (Uuid::parse_str(&room_id), Uuid::parse_str(&user_id)) else {
        return err("StartRoomSession", "id de sala o de usuario inválido");
    };
    let Ok(ticket) = base64::engine::general_purpose::STANDARD.decode(ticket.trim()) else {
        return err("StartRoomSession", "ticket de sala inválido");
    };
    let rendezvous = match resolve(&rendezvous_addr).await {
        Ok(addr) => addr,
        Err(message) => return err("StartRoomSession", message),
    };
    // The relay is optional: without it the mesh just can't fall back.
    let relay = match resolve(&relay_addr).await {
        Ok(addr) => Some(addr),
        Err(message) => {
            warn!(%message, "relay del ayudante no disponible, solo P2P directo");
            None
        }
    };

    let mut s = state.lock().await;
    clear_room_session(&mut s);
    s.game_started.store(false, std::sync::atomic::Ordering::SeqCst);

    let mesh = Arc::new(warlay::engine::PeerMesh::spawn_with(warlay::engine::MeshConfig {
        rendezvous_addr: rendezvous,
        room_token: room_id,
        own_client_id: user_id,
        is_host,
        ticket: Some(ticket),
        relay_addr: relay,
    }));
    s.peer_mesh = Some(mesh.clone());
    s.voice_poll_task = Some(spawn_voice_poll(state.clone(), mesh.clone()));

    let inbound_state = state.clone();
    let inbound_mesh = mesh.clone();
    let lan_inbound_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(20));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let packets = inbound_mesh.poll_lan_packets();
            if packets.is_empty() {
                continue;
            }
            let s = inbound_state.lock().await;
            for (_, bytes) in packets {
                s.lan_loop_guard.mark(&bytes);
                if let Some(tx) = &s.lan_relay_tx {
                    let _ = tx.send(bytes);
                }
            }
        }
    });
    s.room_session = Some(RoomSessionInfo {
        room_id,
        is_host,
        lan_inbound_task,
    });
    info!(%room_id, %user_id, is_host, %rendezvous, ?relay, "sesión de sala P2P iniciada");
    DaemonResponse::Success {
        request_type: "StartRoomSession".to_string(),
    }
}

pub async fn stop_room_session(state: Arc<Mutex<DaemonState>>) -> DaemonResponse {
    let mut s = state.lock().await;
    let had_session = s.room_session.is_some();
    clear_room_session(&mut s);
    drop(s);
    if had_session {
        info!("sesión de sala P2P terminada");
        push_event(&state, DaemonResponse::TunnelStatusChanged { running: false }).await;
    }
    DaemonResponse::Success {
        request_type: "StopRoomSession".to_string(),
    }
}

pub async fn start_game_tunnel(state: Arc<Mutex<DaemonState>>, game_port: u16) -> DaemonResponse {
    let (mesh, room_id, is_host) = {
        let s = state.lock().await;
        match (&s.peer_mesh, &s.room_session) {
            (Some(mesh), Some(session)) => (mesh.clone(), session.room_id, session.is_host),
            _ => return err("StartGameTunnel", "no hay una sesión de sala activa"),
        }
    };

    let join_port = if is_host {
        let state_for_cb = state.clone();
        let handle = warlay::engine::start_hosting_mesh(game_port, mesh, move || {
            let state = state_for_cb.clone();
            tokio::spawn(async move {
                state
                    .lock()
                    .await
                    .game_started
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                push_event(&state, DaemonResponse::GameStarted).await;
            });
        });
        let mut s = state.lock().await;
        if let Some(previous) = s.game_tunnel.replace(handle) {
            previous.abort();
        }
        None
    } else {
        let listen: SocketAddr = (std::net::Ipv4Addr::UNSPECIFIED, 0).into();
        match warlay::engine::start_joining_mesh(room_id, listen, mesh).await {
            Ok((handle, port)) => {
                let mut s = state.lock().await;
                if let Some(previous) = s.game_tunnel.replace(handle) {
                    previous.abort();
                }
                Some(port)
            }
            Err(e) => return err("StartGameTunnel", e),
        }
    };

    if let Err(e) = spawn_lan_mesh(state.clone(), game_port, join_port).await {
        return err("StartGameTunnel", e);
    }
    info!(%room_id, is_host, game_port, ?join_port, "túnel de juego P2P iniciado");
    push_event(&state, DaemonResponse::TunnelStatusChanged { running: true }).await;
    DaemonResponse::Success {
        request_type: "StartGameTunnel".to_string(),
    }
}

pub async fn update_room_helper(
    state: Arc<Mutex<DaemonState>>,
    rendezvous_addr: String,
    relay_addr: String,
) -> DaemonResponse {
    let rendezvous = match resolve(&rendezvous_addr).await {
        Ok(addr) => addr,
        Err(message) => return err("UpdateRoomHelper", message),
    };
    let relay = resolve(&relay_addr).await.ok();
    let s = state.lock().await;
    match &s.peer_mesh {
        Some(mesh) if s.room_session.is_some() => {
            mesh.switch_helper(rendezvous, relay);
            info!(%rendezvous, ?relay, "sala movida a otro ayudante");
            DaemonResponse::Success {
                request_type: "UpdateRoomHelper".to_string(),
            }
        }
        _ => err("UpdateRoomHelper", "no hay una sesión de sala activa"),
    }
}
