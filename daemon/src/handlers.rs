use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use warlay::engine::NetworkError;
use warlay::protocol::ipc::{ConnectFailure, DaemonRequest, DaemonResponse};

use crate::ipc_server::{err, push_event};
use crate::lan::spawn_lan_mesh;
use crate::state::{teardown_tunnel_and_lan, DaemonState};

const HUB_TCP_TUNNEL_PORT: u16 = 7778;

/// Resolves `hub_host` (an IP literal or a hostname) to the tunnel
/// endpoint. With `secure`, the stream is TLS-wrapped and the hub's
/// certificate is checked against `hub_host` itself, not the resolved IP.
async fn hub_tunnel_endpoint(
    hub_host: &str,
    hub_tcp_port: u16,
    secure: bool,
) -> Result<warlay::engine::HubTunnelEndpoint, String> {
    let addr = tokio::net::lookup_host((hub_host, hub_tcp_port))
        .await
        .map_err(|e| format!("no se pudo resolver la dirección del hub '{hub_host}': {e}"))?
        .next()
        .ok_or_else(|| format!("la dirección del hub '{hub_host}' no resolvió a ninguna IP"))?;
    if secure {
        warlay::engine::HubTunnelEndpoint::tls(addr, hub_host).map_err(|e| e.to_string())
    } else {
        Ok(addr.into())
    }
}

fn clear_peer_mesh(s: &mut DaemonState) {
    if let Some(mesh) = s.peer_mesh.take() {
        mesh.abort();
    }
    if let Some(task) = s.voice_poll_task.take() {
        task.abort();
    }
}

pub async fn handle_request(req: DaemonRequest, _state: Arc<Mutex<DaemonState>>) -> DaemonResponse {
    match req {
        DaemonRequest::Ping => DaemonResponse::Pong,

        DaemonRequest::ConnectToHub {
            server_id,
            address,
            psk,
            password,
            display_name,
            client_id,
        } => {
            info!(%address, "connecting to hub");

            let client_id = match Uuid::parse_str(&client_id) {
                Ok(id) => id,
                Err(_) => {
                    warn!(%server_id, "ConnectToHub rechazado: id de cliente inválido");
                    return err("ConnectToHub", "id de cliente inválido");
                }
            };

            // `address` is stored (and shown in the UI) exactly as the user
            // typed it -- legacy bare "host:port", or a ws:// / wss:// URL --
            // but MatchmakingConnection::connect takes the full WebSocket URL
            // of the hub's actual endpoint. HubAddress normalizes either form.
            let hub_address = match warlay::engine::HubAddress::parse(&address) {
                Ok(parsed) => parsed,
                Err(e) => {
                    warn!(%server_id, %address, "ConnectToHub rechazado: dirección inválida");
                    return DaemonResponse::ConnectFailed {
                        server_id,
                        failure: ConnectFailure::Other { message: e.to_string() },
                    };
                }
            };
            let ws_url = hub_address.ws_url().to_string();
            let hub_host = hub_address.host().to_string();

            let server_id_msg = server_id.clone();
            let server_id_ping = server_id.clone();
            let server_id_disc = server_id.clone();

            let hub_host_msg = hub_host.clone();
            let client_id_msg = client_id;

            let state_for_msg = _state.clone();
            let state_for_ping = _state.clone();
            let state_for_lan = _state.clone();
            let state_for_disc = _state.clone();

            let result = warlay::engine::MatchmakingConnection::connect(
                &ws_url,
                client_id,
                psk,
                password,
                move |message| {
                    let server_id_msg_clone = server_id_msg.clone();
                    let hub_host_msg_clone = hub_host_msg.clone();
                    let state_ref = state_for_msg.clone();
                    tokio::spawn(async move {
                        match &message {
                            warlay::protocol::ServerMessage::RoomCreated { room, .. }
                            | warlay::protocol::ServerMessage::JoinedRoom { room, .. } => {
                                info!(server_id = %server_id_msg_clone, room_id = %room.id, "sala activa establecida");
                                let mut s = state_ref.lock().await;
                                s.active_room_server = Some(server_id_msg_clone.clone());
                                s.active_room_id = Some(room.id);

                                clear_peer_mesh(&mut s);

                                let rv_port = HUB_TCP_TUNNEL_PORT + 1;
                                let rv_str = format!("{hub_host_msg_clone}:{rv_port}");
                                let rendezvous_addr =
                                    std::net::ToSocketAddrs::to_socket_addrs(&(hub_host_msg_clone.as_str(), rv_port))
                                        .ok()
                                        .and_then(|mut iter| iter.next());

                                if let Some(rv_addr) = rendezvous_addr {
                                    let is_host = room.host_client_id == client_id_msg;
                                    let mesh =
                                        warlay::engine::PeerMesh::spawn(rv_addr, room.id, client_id_msg, is_host);
                                    let mesh_handle = Arc::new(mesh);
                                    s.peer_mesh = Some(mesh_handle.clone());

                                    let state_for_voice = state_ref.clone();
                                    let voice_task = tokio::spawn(async move {
                                        let mut interval = tokio::time::interval(std::time::Duration::from_millis(15));
                                        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                                        loop {
                                            interval.tick().await;
                                            let datagrams = mesh_handle.poll_datagrams();
                                            for (from_client_id, opus_bytes) in datagrams {
                                                push_event(
                                                    &state_for_voice,
                                                    DaemonResponse::VoiceFrameIn {
                                                        from_client_id: from_client_id.to_string(),
                                                        opus_bytes,
                                                    },
                                                )
                                                .await;
                                            }
                                        }
                                    });
                                    s.voice_poll_task = Some(voice_task);
                                } else {
                                    warn!(%rv_str, "no se pudo resolver dirección de rendezvous para PeerMesh");
                                }
                            }
                            warlay::protocol::ServerMessage::RoomRemoved { room_id } => {
                                let mut s = state_ref.lock().await;
                                // Keyed on the room this message is actually
                                // about, not just "is there some active room
                                // on this server" -- see `active_room_id`'s
                                // doc comment in `state.rs` for why: this
                                // message can arrive late, after a newer
                                // `RoomCreated` for a *different* room has
                                // already landed, and must not clear that
                                // newer room's state out from under it.
                                if s.active_room_server == Some(server_id_msg_clone.clone())
                                    && s.active_room_id == Some(*room_id)
                                {
                                    info!(server_id = %server_id_msg_clone, %room_id, "sala activa despejada");
                                    s.active_room_server = None;
                                    s.active_room_id = None;
                                    clear_peer_mesh(&mut s);
                                }
                            }
                            warlay::protocol::ServerMessage::LeftRoom => {
                                let mut s = state_ref.lock().await;
                                if s.active_room_server == Some(server_id_msg_clone.clone()) {
                                    info!(server_id = %server_id_msg_clone, "sala activa despejada");
                                    s.active_room_server = None;
                                    s.active_room_id = None;
                                    clear_peer_mesh(&mut s);
                                }
                            }
                            _ => {}
                        }
                        push_event(
                            &state_ref,
                            DaemonResponse::HubMessage {
                                server_id: server_id_msg_clone,
                                message,
                            },
                        )
                        .await;
                    });
                },
                move |latency| {
                    let server_id_ping_clone = server_id_ping.clone();
                    let state_ref = state_for_ping.clone();
                    tokio::spawn(async move {
                        push_event(
                            &state_ref,
                            DaemonResponse::HubPing {
                                server_id: server_id_ping_clone,
                                latency_ms: latency as u32,
                            },
                        )
                        .await;
                    });
                },
                move |bytes| {
                    let state_ref = state_for_lan.clone();
                    tokio::spawn(async move {
                        let s = state_ref.lock().await;
                        s.lan_loop_guard.mark(&bytes);
                        if let Some(tx) = &s.lan_relay_tx {
                            let _ = tx.send(bytes);
                        }
                    });
                },
                move || {
                    let server_id_disc_clone = server_id_disc.clone();
                    let state_ref = state_for_disc.clone();
                    tokio::spawn(async move {
                        warn!(server_id = %server_id_disc_clone, "conexión con el hub perdida");
                        push_event(
                            &state_ref,
                            DaemonResponse::HubConnectionState {
                                server_id: server_id_disc_clone,
                                connected: false,
                            },
                        )
                        .await;
                    });
                },
            )
            .await;

            match result {
                Ok(outcome) => {
                    info!(%server_id, remote_server_id = %outcome.server_id, server_name = %outcome.server_name, "conectado al hub");
                    let mut s = _state.lock().await;
                    outcome
                        .connection
                        .send(warlay::protocol::ClientMessage::Authenticate { display_name });
                    s.connections.insert(server_id.clone(), outcome.connection);
                    drop(s);
                    push_event(
                        &_state,
                        DaemonResponse::HubConnectionState {
                            server_id: server_id.clone(),
                            connected: true,
                        },
                    )
                    .await;
                    DaemonResponse::Connected {
                        server_id,
                        remote_server_id: outcome.server_id.to_string(),
                        server_name: outcome.server_name,
                        new_psk: outcome.new_psk,
                    }
                }
                Err(e) => {
                    let failure = match e {
                        NetworkError::PasswordRequired { server_name } => {
                            ConnectFailure::PasswordRequired { server_name }
                        }
                        NetworkError::AuthRejected { message, retry_as_new } => {
                            ConnectFailure::Rejected { message, retry_as_new }
                        }
                        other => ConnectFailure::Other {
                            message: other.to_string(),
                        },
                    };
                    warn!(%server_id, ?failure, "fallo al conectar al hub");
                    DaemonResponse::ConnectFailed { server_id, failure }
                }
            }
        }

        DaemonRequest::DisconnectFromHub { server_id } => {
            info!(%server_id, "disconnecting from hub");
            let mut s = _state.lock().await;
            s.connections.remove(&server_id);
            if s.active_room_server == Some(server_id.clone()) {
                s.active_room_server = None;
                s.active_room_id = None;
                clear_peer_mesh(&mut s);
                teardown_tunnel_and_lan(&mut s);
            }
            DaemonResponse::Success {
                request_type: "DisconnectFromHub".to_string(),
            }
        }

        DaemonRequest::DisconnectAll => {
            let mut s = _state.lock().await;
            let server_ids: Vec<String> = s.connections.keys().cloned().collect();
            info!(
                server_count = server_ids.len(),
                ?server_ids,
                "disconnecting from all hubs (app quitting)"
            );
            // Dropping each `MatchmakingConnection` aborts its reader/writer/
            // ping tasks (see its `Drop` impl), closing the underlying
            // socket -- that's what actually lets the hub notice the
            // disconnect and update everyone else's view in real time.
            s.connections.clear();
            s.active_room_server = None;
            s.active_room_id = None;
            clear_peer_mesh(&mut s);
            teardown_tunnel_and_lan(&mut s);
            DaemonResponse::Success {
                request_type: "DisconnectAll".to_string(),
            }
        }

        DaemonRequest::JoinRoom {
            server_id,
            room_id,
            password,
        } => {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(id) => id,
                Err(_) => return err("JoinRoom", "id de sala inválido"),
            };
            let s = _state.lock().await;
            match s.connections.get(&server_id) {
                Some(conn) => {
                    info!(%server_id, %room_id, "uniéndose a sala");
                    conn.send(warlay::protocol::ClientMessage::JoinRoom {
                        room_id: room_uuid,
                        password,
                    });
                    DaemonResponse::Success {
                        request_type: "JoinRoom".to_string(),
                    }
                }
                None => {
                    warn!(%server_id, %room_id, "JoinRoom sin conexión activa con ese servidor");
                    err("JoinRoom", "no hay una conexión activa con ese servidor")
                }
            }
        }

        DaemonRequest::LeaveRoom { server_id } => {
            let mut s = _state.lock().await;
            let Some(conn) = s.connections.get(&server_id) else {
                warn!(%server_id, "LeaveRoom sin conexión activa con ese servidor");
                return err("LeaveRoom", "no hay una conexión activa con ese servidor");
            };
            info!(%server_id, "saliendo de la sala");
            conn.send(warlay::protocol::ClientMessage::LeaveRoom);
            if s.active_room_server == Some(server_id) {
                s.active_room_server = None;
                s.active_room_id = None;
                clear_peer_mesh(&mut s);
                teardown_tunnel_and_lan(&mut s);
            }
            DaemonResponse::Success {
                request_type: "LeaveRoom".to_string(),
            }
        }

        DaemonRequest::CreateRoom {
            server_id,
            name,
            game_port,
            password,
        } => {
            let s = _state.lock().await;
            match s.connections.get(&server_id) {
                Some(conn) => {
                    info!(%server_id, %name, game_port, "creando sala");
                    conn.send(warlay::protocol::ClientMessage::CreateRoom {
                        name,
                        game_port,
                        password,
                    });
                    DaemonResponse::Success {
                        request_type: "CreateRoom".to_string(),
                    }
                }
                None => {
                    warn!(%server_id, "CreateRoom sin conexión activa con ese servidor");
                    err("CreateRoom", "no hay una conexión activa con ese servidor")
                }
            }
        }

        DaemonRequest::DeleteRoom { server_id, room_id } => {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(id) => id,
                Err(_) => return err("DeleteRoom", "id de sala inválido"),
            };
            let mut s = _state.lock().await;
            let Some(conn) = s.connections.get(&server_id) else {
                warn!(%server_id, %room_id, "DeleteRoom sin conexión activa con ese servidor");
                return err("DeleteRoom", "no hay una conexión activa con ese servidor");
            };
            info!(%server_id, %room_id, "eliminando sala");
            conn.send(warlay::protocol::ClientMessage::DeleteRoom { room_id: room_uuid });
            if s.active_room_server == Some(server_id) {
                s.active_room_server = None;
                s.active_room_id = None;
                clear_peer_mesh(&mut s);
                teardown_tunnel_and_lan(&mut s);
            }
            DaemonResponse::Success {
                request_type: "DeleteRoom".to_string(),
            }
        }

        DaemonRequest::SetRoomInProgress { server_id } => {
            let s = _state.lock().await;
            match s.connections.get(&server_id) {
                Some(conn) => {
                    info!(%server_id, "marcando sala en partida");
                    conn.send(warlay::protocol::ClientMessage::SetRoomInProgress);
                    DaemonResponse::Success {
                        request_type: "SetRoomInProgress".to_string(),
                    }
                }
                None => {
                    warn!(%server_id, "SetRoomInProgress sin conexión activa con ese servidor");
                    err("SetRoomInProgress", "no hay una conexión activa con ese servidor")
                }
            }
        }

        DaemonRequest::SetRoomAvailable { server_id } => {
            let s = _state.lock().await;
            match s.connections.get(&server_id) {
                Some(conn) => {
                    info!(%server_id, "marcando sala disponible");
                    conn.send(warlay::protocol::ClientMessage::SetRoomAvailable);
                    DaemonResponse::Success {
                        request_type: "SetRoomAvailable".to_string(),
                    }
                }
                None => {
                    warn!(%server_id, "SetRoomAvailable sin conexión activa con ese servidor");
                    err("SetRoomAvailable", "no hay una conexión activa con ese servidor")
                }
            }
        }

        DaemonRequest::SendChatMessage { server_id, text } => {
            let s = _state.lock().await;
            match s.connections.get(&server_id) {
                Some(conn) => {
                    // Nunca se registra el contenido del mensaje, solo que se envió uno.
                    debug!(%server_id, "enviando mensaje de chat");
                    conn.send(warlay::protocol::ClientMessage::ChatMessage { text });
                    DaemonResponse::Success {
                        request_type: "SendChatMessage".to_string(),
                    }
                }
                None => {
                    warn!(%server_id, "SendChatMessage sin conexión activa con ese servidor");
                    err("SendChatMessage", "no hay una conexión activa con ese servidor")
                }
            }
        }

        DaemonRequest::SendPrivateMessage {
            server_id,
            to_client_id,
            text,
        } => {
            let to_uuid = match uuid::Uuid::parse_str(&to_client_id) {
                Ok(id) => id,
                Err(_) => return err("SendPrivateMessage", "id de destinatario inválido"),
            };
            let s = _state.lock().await;
            match s.connections.get(&server_id) {
                Some(conn) => {
                    // Nunca se registra el contenido del mensaje, solo que se envió uno.
                    debug!(%server_id, %to_client_id, "enviando mensaje privado");
                    conn.send(warlay::protocol::ClientMessage::PrivateMessage {
                        to_client_id: to_uuid,
                        text,
                    });
                    DaemonResponse::Success {
                        request_type: "SendPrivateMessage".to_string(),
                    }
                }
                None => {
                    warn!(%server_id, "SendPrivateMessage sin conexión activa con ese servidor");
                    err("SendPrivateMessage", "no hay una conexión activa con ese servidor")
                }
            }
        }

        DaemonRequest::StartHostingTunnel {
            hub_host,
            hub_tcp_port,
            room_token,
            game_port,
            secure,
        } => {
            let hub_tcp_addr = match hub_tunnel_endpoint(&hub_host, hub_tcp_port, secure).await {
                Ok(endpoint) => endpoint,
                Err(message) => return err("StartHostingTunnel", message),
            };
            let room_token = match uuid::Uuid::parse_str(&room_token) {
                Ok(t) => t,
                Err(_) => return err("StartHostingTunnel", "token de sala inválido"),
            };

            let mesh = _state.lock().await.peer_mesh.clone();
            let state_for_cb = _state.clone();
            let handle = match warlay::engine::start_hosting_with_p2p(
                hub_tcp_addr.clone(),
                room_token,
                game_port,
                mesh,
                move || {
                    let state_clone = state_for_cb.clone();
                    tokio::spawn(async move {
                        let s = state_clone.lock().await;
                        if let Some(active_id) = &s.active_room_server {
                            if let Some(conn) = s.connections.get(active_id) {
                                conn.send(warlay::protocol::ClientMessage::SetRoomInProgress);
                            }
                        }
                        s.game_started.store(true, std::sync::atomic::Ordering::SeqCst);
                    });
                },
            )
            .await
            {
                Ok(handle) => handle,
                Err(e) => {
                    error!(error = %e, "failed to start hosting tunnel");
                    return err("StartHostingTunnel", e);
                }
            };

            {
                let mut s = _state.lock().await;
                if let Some(previous) = s.game_tunnel.take() {
                    previous.abort();
                }
                s.game_tunnel = Some(handle);
            }

            if let Err(e) = spawn_lan_mesh(_state.clone(), game_port, None).await {
                error!(error = %e, %hub_tcp_addr, %room_token, "failed to start LAN mesh while hosting");
                return err("StartHostingTunnel", e);
            }

            info!(%hub_tcp_addr, %room_token, game_port, "hosting tunnel started");
            push_event(&_state, DaemonResponse::TunnelStatusChanged { running: true }).await;
            DaemonResponse::Success {
                request_type: "StartHostingTunnel".to_string(),
            }
        }

        DaemonRequest::StartJoiningTunnel {
            hub_host,
            hub_tcp_port,
            room_token,
            game_port,
            secure,
        } => {
            let hub_tcp_addr = match hub_tunnel_endpoint(&hub_host, hub_tcp_port, secure).await {
                Ok(endpoint) => endpoint,
                Err(message) => return err("StartJoiningTunnel", message),
            };
            let room_token = match uuid::Uuid::parse_str(&room_token) {
                Ok(t) => t,
                Err(_) => return err("StartJoiningTunnel", "token de sala inválido"),
            };
            let local_listen_addr = std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0);

            let mesh = _state.lock().await.peer_mesh.clone();
            let (tunnel_handle, actual_port) =
                match warlay::engine::start_joining_with_p2p(hub_tcp_addr.clone(), room_token, local_listen_addr, mesh)
                    .await
                {
                    Ok(result) => result,
                    Err(e) => {
                        error!(error = %e, "failed to start joining tunnel");
                        return err("StartJoiningTunnel", e);
                    }
                };

            {
                let mut s = _state.lock().await;
                if let Some(previous) = s.game_tunnel.take() {
                    previous.abort();
                }
                s.game_tunnel = Some(tunnel_handle);
            }

            if let Err(e) = spawn_lan_mesh(_state.clone(), game_port, Some(actual_port)).await {
                error!(error = %e, %hub_tcp_addr, %room_token, "failed to start LAN mesh while joining");
                return err("StartJoiningTunnel", e);
            }

            info!(%hub_tcp_addr, %room_token, game_port, actual_port, "joining tunnel started");
            push_event(&_state, DaemonResponse::TunnelStatusChanged { running: true }).await;
            DaemonResponse::Success {
                request_type: "StartJoiningTunnel".to_string(),
            }
        }

        DaemonRequest::StopHubTunnel => {
            info!("stopping hub tunnel");
            let mut s = _state.lock().await;
            teardown_tunnel_and_lan(&mut s);
            drop(s);
            push_event(&_state, DaemonResponse::TunnelStatusChanged { running: false }).await;
            DaemonResponse::Success {
                request_type: "StopHubTunnel".to_string(),
            }
        }

        DaemonRequest::LaunchGame { path, args } => {
            let exe_path = std::path::Path::new(&path);
            let has_exe_extension = exe_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("exe"))
                .unwrap_or(false);
            if !has_exe_extension {
                return err("LaunchGame", "la ruta debe apuntar a un ejecutable (.exe)");
            }
            match exe_path.metadata() {
                Ok(meta) if meta.is_file() => {}
                Ok(_) => return err("LaunchGame", "la ruta no es un archivo"),
                Err(_) => return err("LaunchGame", "el ejecutable no existe"),
            }

            // The daemon runs elevated (via the logon-scheduled task -- see
            // main()'s docs), and a spawned child normally inherits that
            // elevated token, so the launched game runs elevated too. Full
            // de-elevation (CreateProcessAsUser with a stripped-integrity
            // token) is a real technique but a substantial addition for a
            // narrow risk: `path` is the user's own configured executable,
            // reached only through a pipe now restricted to interactive
            // users/SYSTEM/Administrators AND to this installation's own
            // warlay.exe process (see security::verify_client_process), not
            // attacker-controlled input. Flagged deliberately rather than
            // silently accepted -- a documented follow-up, not solved here.
            info!(%path, "launching game");
            if let Err(e) = std::process::Command::new(&path).args(&args).spawn() {
                error!(error = %e, %path, "failed to launch game");
                return err("LaunchGame", format!("no se pudo iniciar el juego: {e}"));
            }
            DaemonResponse::Success {
                request_type: "LaunchGame".to_string(),
            }
        }

        DaemonRequest::VoiceFrameOut { room_token, opus_bytes } => {
            let peer_mesh = _state.lock().await.peer_mesh.clone();
            if let Some(mesh) = peer_mesh {
                let connected = mesh.connected_peers();
                debug!(%room_token, peer_count = connected.len(), bytes_len = opus_bytes.len(), "enviando frame de voz a peers");
                for (peer_id, _) in connected {
                    mesh.send_datagram_to(peer_id, opus_bytes.clone());
                }
            }
            DaemonResponse::Success {
                request_type: "VoiceFrameOut".to_string(),
            }
        }

        DaemonRequest::GetAgentSettings => {
            let settings = _state.lock().await.agent_store.get_settings();
            DaemonResponse::AgentSettings { settings }
        }

        DaemonRequest::SetAgentSettings { settings } => {
            if let Err(e) = crate::log_control::set_level(&settings.log_level) {
                return err("SetAgentSettings", format!("nivel de log inválido: {e}"));
            }
            let s = _state.lock().await;
            s.lan_discovery_enabled
                .store(settings.lan_discovery_enabled, std::sync::atomic::Ordering::Relaxed);
            if let Err(e) = s.agent_store.save_settings(&settings) {
                return err(
                    "SetAgentSettings",
                    format!("no se pudo guardar la configuración del agente: {e}"),
                );
            }
            info!(log_level = %settings.log_level, lan_discovery_enabled = settings.lan_discovery_enabled, "configuración del agente actualizada");
            DaemonResponse::Success {
                request_type: "SetAgentSettings".to_string(),
            }
        }

        DaemonRequest::GetAgentStatus => {
            let s = _state.lock().await;
            let mesh_peers = s
                .peer_mesh
                .as_ref()
                .map(|mesh| {
                    mesh.peer_statuses()
                        .into_iter()
                        .map(
                            |(client_id, is_host, p2p_addr, state)| warlay::protocol::ipc::PeerLinkStatus {
                                client_id: client_id.to_string(),
                                is_host,
                                p2p_addr: p2p_addr.map(|a| a.to_string()),
                                state: state.to_string(),
                            },
                        )
                        .collect()
                })
                .unwrap_or_default();

            DaemonResponse::AgentStatus {
                status: warlay::protocol::ipc::AgentStatusSnapshot {
                    uptime_secs: s.started_at.elapsed().as_secs(),
                    hub_connections: s.connections.len(),
                    tunnel_active: s.game_tunnel.is_some(),
                    lan_discovery_enabled: s.lan_discovery_enabled.load(std::sync::atomic::Ordering::Relaxed),
                    mesh_peers,
                },
            }
        }

        DaemonRequest::RetryPeerMesh => {
            let peer_mesh = _state.lock().await.peer_mesh.clone();
            match peer_mesh {
                Some(mesh) => {
                    info!("reintentando conexión P2P por pedido manual desde el panel Agente");
                    mesh.retry_failed_peers();
                    DaemonResponse::Success {
                        request_type: "RetryPeerMesh".to_string(),
                    }
                }
                None => err(
                    "RetryPeerMesh",
                    "no hay una malla P2P activa (¿hay una sala abierta ahora mismo?)",
                ),
            }
        }

        DaemonRequest::StartRoomSession {
            room_id,
            user_id,
            is_host,
            rendezvous_addr,
            relay_addr,
            ticket,
        } => {
            crate::session::start_room_session(_state, room_id, user_id, is_host, rendezvous_addr, relay_addr, ticket)
                .await
        }

        DaemonRequest::StopRoomSession => crate::session::stop_room_session(_state).await,

        DaemonRequest::StartGameTunnel { game_port } => crate::session::start_game_tunnel(_state, game_port).await,

        DaemonRequest::UpdateRoomHelper {
            rendezvous_addr,
            relay_addr,
        } => crate::session::update_room_helper(_state, rendezvous_addr, relay_addr).await,
    }
}
