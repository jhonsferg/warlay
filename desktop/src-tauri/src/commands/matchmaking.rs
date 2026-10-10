use crate::core::dto::ConnectStatusDto;
use crate::core::error::AppError;
use crate::daemon_ipc::DaemonClient;
use crate::state::AppState;
use tauri::{AppHandle, State};
use uuid::Uuid;
use warlay::protocol::ipc::{ConnectFailure, DaemonRequest, DaemonResponse};

fn invalid(field: &str, value: &str) -> AppError {
    AppError::InvalidInput(format!("'{value}' is not a valid {field}"))
}

/// Every command below needs a live daemon connection to do anything at
/// all; silently no-op'ing when there isn't one (the previous behavior)
/// hides real failures from the user instead of surfacing them.
pub(crate) fn require_client(state: &State<'_, AppState>) -> Result<DaemonClient, AppError> {
    state
        .daemon_client
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::Any(anyhow::anyhow!("no hay conexión activa con el daemon")))
}

/// Maps the daemon's generic Success/Error reply to a Tauri command result.
pub(crate) fn success_or_err(resp: DaemonResponse) -> Result<(), AppError> {
    match resp {
        DaemonResponse::Success { .. } => Ok(()),
        DaemonResponse::Error { message, .. } => Err(AppError::Any(anyhow::anyhow!(message))),
        other => Err(AppError::Any(anyhow::anyhow!(
            "respuesta inesperada del daemon: {other:?}"
        ))),
    }
}

#[tauri::command]
pub async fn connect_to_server(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
    password: Option<String>,
) -> Result<ConnectStatusDto, AppError> {
    let local_id = Uuid::parse_str(&server_id).map_err(|_| invalid("server id", &server_id))?;
    let record = state
        .store
        .get_server(local_id)?
        .ok_or_else(|| AppError::InvalidInput("servidor no encontrado".to_string()))?;
    let display_name = state.store.get_settings()?.display_name.clone();

    // Normally already true by the time this runs -- the startup loading
    // screen calls ensure_daemon_running before showing the rest of the UI
    // -- but call it again as a safety net (e.g. the daemon crashed since
    // then) rather than silently spawning a non-elevated one here, which
    // used to look healthy right up until the first LAN capture attempt.
    crate::commands::daemon::ensure_daemon_running(app.clone(), state.clone()).await?;
    let client = state
        .daemon_client
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::Any(anyhow::anyhow!("no hay conexión activa con el daemon")))?;

    let response = client
        .call(DaemonRequest::ConnectToHub {
            server_id: server_id.clone(),
            address: record.address,
            psk: record.psk,
            password,
            display_name,
            client_id: state.store.client_id().to_string(),
        })
        .await
        .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?;

    match response {
        DaemonResponse::Connected {
            remote_server_id,
            server_name,
            new_psk,
            ..
        } => {
            let remote_id = Uuid::parse_str(&remote_server_id)
                .map_err(|_| AppError::Any(anyhow::anyhow!("id de servidor inválido recibido del daemon")))?;
            state
                .store
                .record_handshake_success(local_id, remote_id, &server_name, new_psk.as_ref())?;
            Ok(ConnectStatusDto::Connected { server_name })
        }
        DaemonResponse::ConnectFailed { failure, .. } => match failure {
            ConnectFailure::PasswordRequired { server_name } => Ok(ConnectStatusDto::PasswordRequired { server_name }),
            ConnectFailure::Rejected { message, retry_as_new } => {
                if retry_as_new {
                    // The server rejected our stored key outright -- keeping
                    // it around would just make the next attempt fail the
                    // same way.
                    state.store.clear_server_psk(local_id)?;
                }
                Ok(ConnectStatusDto::Rejected { message, retry_as_new })
            }
            ConnectFailure::Other { message } => Err(AppError::Any(anyhow::anyhow!(message))),
        },
        DaemonResponse::Error { message, .. } => Err(AppError::Any(anyhow::anyhow!(message))),
        other => Err(AppError::Any(anyhow::anyhow!(
            "respuesta inesperada del daemon: {other:?}"
        ))),
    }
}

#[tauri::command]
pub async fn disconnect_from_server(state: State<'_, AppState>, server_id: String) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::DisconnectFromHub { server_id })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

#[tauri::command]
pub async fn create_room(
    state: State<'_, AppState>,
    server_id: String,
    name: String,
    game_port: u16,
    password: Option<String>,
) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::CreateRoom {
                server_id,
                name,
                game_port,
                password,
            })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

#[tauri::command]
pub async fn join_room(
    state: State<'_, AppState>,
    server_id: String,
    room_id: String,
    password: Option<String>,
) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::JoinRoom {
                server_id,
                room_id,
                password,
            })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

#[tauri::command]
pub async fn leave_room(state: State<'_, AppState>, server_id: String) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::LeaveRoom { server_id })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

#[tauri::command]
pub async fn delete_room(state: State<'_, AppState>, server_id: String, room_id: String) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::DeleteRoom { server_id, room_id })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

#[tauri::command]
pub async fn set_room_in_progress(state: State<'_, AppState>, server_id: String) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::SetRoomInProgress { server_id })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

#[tauri::command]
pub async fn set_room_available(state: State<'_, AppState>, server_id: String) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::SetRoomAvailable { server_id })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

#[tauri::command]
pub async fn send_chat_message(state: State<'_, AppState>, server_id: String, text: String) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::SendChatMessage { server_id, text })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

#[tauri::command]
pub async fn send_private_message(
    state: State<'_, AppState>,
    server_id: String,
    to_client_id: String,
    text: String,
) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::SendPrivateMessage {
                server_id,
                to_client_id,
                text,
            })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

/// Starts the room host's side of the real game-session tunnel: a control
/// connection to the hub's TCP tunnel port that, on demand, bridges a
/// joiner's stream to this machine's real WC3 host listener on
/// `127.0.0.1:game_port`. Also joins the room's LAN mesh (see
/// `spawn_lan_mesh`) — this matters even for the host: WC3's own game
/// announcement is often sent only in *response* to a `SEARCHGAME` it
/// receives, not proactively, so the host needs to actually receive a
/// joiner's relayed search broadcast on its own network before its WC3 has
/// any reason to answer with the real `GAMEINFO`
#[tauri::command]
pub async fn start_hosting_tunnel(
    state: State<'_, AppState>,
    hub_host: String,
    hub_tcp_port: u16,
    room_token: String,
    game_port: u16,
    secure: bool,
) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::StartHostingTunnel {
                hub_host,
                hub_tcp_port,
                room_token,
                game_port,
                secure,
            })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

#[tauri::command]
pub async fn start_joining_tunnel(
    state: State<'_, AppState>,
    hub_host: String,
    hub_tcp_port: u16,
    room_token: String,
    game_port: u16,
    secure: bool,
) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::StartJoiningTunnel {
                hub_host,
                hub_tcp_port,
                room_token,
                game_port,
                secure,
            })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

#[tauri::command]
pub async fn stop_hub_tunnel(state: State<'_, AppState>) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::StopHubTunnel)
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

/// Current truth for the active room's member roster, kept live by
/// `daemon_ipc.rs` (see `AppState::active_room`'s doc comment) -- a plain
/// local state read, no daemon round-trip needed. `OverlayApp.vue` calls
/// this on mount as a fallback for whatever membership changes happened
/// *before* the overlay window existed, since it can't read the main
/// window's Pinia store (separate JS realm).
#[tauri::command]
pub fn get_room_members(state: State<'_, AppState>) -> Vec<warlay::protocol::RoomMember> {
    state
        .active_room
        .lock()
        .unwrap()
        .as_ref()
        .map(|(_, members)| members.clone())
        .unwrap_or_default()
}
