//! Room sessions of the new architecture: the main server hands the UI a
//! room ticket and the helper assigned to the room; the agent joins the
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

use warlay::protocol::ipc::AgentResponse;

use super::events::{emit_event, err};
use super::lan::spawn_lan_mesh;
use super::state::{teardown_tunnel_and_lan, AgentState};

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
pub fn clear_room_session(s: &mut AgentState) {
    if let Some(mesh) = s.peer_mesh.take() {
        mesh.abort();
    }
    s.room_session = None;
    teardown_tunnel_and_lan(s);
}

#[allow(clippy::too_many_arguments)]
pub async fn start_room_session(
    state: Arc<Mutex<AgentState>>,
    room_id: String,
    user_id: String,
    is_host: bool,
    rendezvous_addr: String,
    relay_addr: String,
    ticket: String,
) -> AgentResponse {
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

    let inbound_state = state.clone();
    let inbound_mesh = mesh.clone();
    let lan_inbound_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(20));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            // Voice rides the same 20 ms tick: one batched event per tick.
            super::voice::emit_voice_frames(inbound_mesh.poll_voice_frames());
            super::voice::emit_desktop_frames(inbound_mesh.poll_desktop_frames());
            let packets = inbound_mesh.poll_lan_packets();
            if packets.is_empty() {
                continue;
            }
            // Snapshot what each packet needs while holding the lock; the
            // gateway bind is async work that must not hold it.
            let (bridge, relay_tx, gateway) = {
                let s = inbound_state.lock().await;
                (s.lan_bridge(), s.lan_relay_tx.clone(), s.game_gateway.clone())
            };
            for (from, bytes) in packets {
                // The replay rules (echo marking, host gate, per-peer
                // GAMEINFO port rewrite) live in `warlay::engine::LanBridge`
                // so the lab simulator and the tests run the same code.
                let Some(bytes) = bridge.prepare_inbound(from, bytes, gateway.as_deref()).await else {
                    continue;
                };
                if let Some(tx) = &relay_tx {
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
    AgentResponse::Success {
        request_type: "StartRoomSession".to_string(),
    }
}

pub async fn stop_room_session(state: Arc<Mutex<AgentState>>) -> AgentResponse {
    let mut s = state.lock().await;
    let had_session = s.room_session.is_some();
    clear_room_session(&mut s);
    drop(s);
    if had_session {
        info!("sesión de sala P2P terminada");
        emit_event(&AgentResponse::TunnelStatusChanged { running: false });
    }
    AgentResponse::Success {
        request_type: "StopRoomSession".to_string(),
    }
}

/// Starts this member's game tunnel. Both halves on every member, host or
/// not: anyone may host a game the others join, and anyone may join a
/// game another member hosts.
pub async fn start_game_tunnel(state: Arc<Mutex<AgentState>>, game_port: u16) -> AgentResponse {
    let (mesh, room_id, is_host) = {
        let s = state.lock().await;
        match (&s.peer_mesh, &s.room_session) {
            (Some(mesh), Some(session)) => (mesh.clone(), session.room_id, session.is_host),
            _ => return err("StartGameTunnel", "no hay una sesión de sala activa"),
        }
    };

    // Host side: register our local WC3 on the mesh so any member's joiner
    // gateway can carry a session to it (`STREAM_LOCAL_GAME`, directly or
    // through a member that has our link).
    let state_for_cb = state.clone();
    let host_handle = warlay::engine::start_hosting_mesh(game_port, mesh.clone(), move || {
        let state = state_for_cb.clone();
        tokio::spawn(async move {
            state
                .lock()
                .await
                .game_started
                .store(true, std::sync::atomic::Ordering::SeqCst);
            emit_event(&AgentResponse::GameStarted);
        });
    });
    // Join side: one dedicated local port per peer, bound on demand as
    // their rewritten GAMEINFOs arrive — so a connection there says
    // exactly which member's game our WC3 is asking to join.
    let gateway = Arc::new(warlay::engine::start_game_gateway(room_id, mesh));

    {
        let mut s = state.lock().await;
        if let Some(previous) = s.game_tunnel.replace(host_handle) {
            previous.abort();
        }
        if let Some(previous) = s.game_gateway.replace(gateway) {
            previous.abort();
        }
    }

    if let Err(e) = spawn_lan_mesh(state.clone(), game_port, None).await {
        return err("StartGameTunnel", e);
    }
    info!(%room_id, is_host, game_port, "túnel de juego P2P iniciado");
    emit_event(&AgentResponse::TunnelStatusChanged { running: true });
    AgentResponse::Success {
        request_type: "StartGameTunnel".to_string(),
    }
}

pub async fn update_room_helper(
    state: Arc<Mutex<AgentState>>,
    rendezvous_addr: String,
    relay_addr: String,
) -> AgentResponse {
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
            AgentResponse::Success {
                request_type: "UpdateRoomHelper".to_string(),
            }
        }
        _ => err("UpdateRoomHelper", "no hay una sesión de sala activa"),
    }
}
