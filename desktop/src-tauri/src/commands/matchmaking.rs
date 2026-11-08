use std::sync::Arc;
use tauri::async_runtime::Mutex as TokioMutex;

use crate::agent::handlers::handle_request;
use crate::agent::state::DaemonState;
use crate::core::dto::ConnectStatusDto;
use crate::core::error::AppError;
use crate::state::AppState;
use anyhow::anyhow;
use tauri::{AppHandle, State};
use uuid::Uuid;
use warlay::protocol::ipc::{ConnectFailure, DaemonRequest, DaemonResponse};

fn invalid(field: &str, value: &str) -> AppError {
    AppError::InvalidInput(format!("'{value}' is not a valid {field}"))
}

/// Every command below needs the agent to be running; silently no-op'ing
/// when it isn't hides real failures from the user instead of surfacing
/// them.
pub(crate) fn require_agent_state(state: &State<'_, AppState>) -> Result<Arc<TokioMutex<DaemonState>>, AppError> {
    state
        .agent_state
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::Any(anyhow!("el agente no está iniciado")))
}

/// Maps the agent's generic Success/Error reply to a Tauri command result.
pub(crate) fn success_or_err(resp: DaemonResponse) -> Result<(), AppError> {
    match resp {
        DaemonResponse::Success { .. } => Ok(()),
        DaemonResponse::Error { message, .. } => Err(AppError::Any(anyhow!(message))),
        other => Err(AppError::Any(anyhow!("respuesta inesperada del agente: {other:?}"))),
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

    let agent_state = require_agent_state(&state)?;

    let response = handle_request(
        DaemonRequest::ConnectToHub {
            server_id: server_id.clone(),
            address: record.address,
            psk: record.psk,
            password,
            display_name,
            client_id: state.store.client_id().to_string(),
        },
        agent_state,
        app,
    )
    .await;

    match response {
        DaemonResponse::Connected {
            remote_server_id,
            server_name,
            new_psk,
            ..
        } => {
            let remote_id = Uuid::parse_str(&remote_server_id)
                .map_err(|_| AppError::Any(anyhow::anyhow!("id de servidor inválido recibido del agente")))?;
            state
                .store
                .record_handshake_success(local_id, remote_id, &server_name, new_psk.as_ref())?;
            Ok(ConnectStatusDto::Connected { server_name })
        }
        DaemonResponse::ConnectFailed { failure, .. } => match failure {
            ConnectFailure::PasswordRequired { server_name } => Ok(ConnectStatusDto::PasswordRequired { server_name }),
            ConnectFailure::Rejected { message, retry_as_new } => {
                if retry_as_new {
                    state.store.clear_server_psk(local_id)?;
                }
                Ok(ConnectStatusDto::Rejected { message, retry_as_new })
            }
            ConnectFailure::Other { message } => Err(AppError::Any(anyhow::anyhow!(message))),
        },
        DaemonResponse::Error { message, .. } => Err(AppError::Any(anyhow::anyhow!(message))),
        other => Err(AppError::Any(anyhow!("respuesta inesperada del agente: {other:?}"))),
    }
}

#[tauri::command]
pub async fn disconnect_from_server(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(handle_request(DaemonRequest::DisconnectFromHub { server_id }, agent_state, app).await)
}

#[tauri::command]
pub async fn create_room(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
    name: String,
    game_port: u16,
    password: Option<String>,
) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(
        handle_request(
            DaemonRequest::CreateRoom {
                server_id,
                name,
                game_port,
                password,
            },
            agent_state,
            app,
        )
        .await,
    )
}

#[tauri::command]
pub async fn join_room(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
    room_id: String,
    password: Option<String>,
) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(
        handle_request(
            DaemonRequest::JoinRoom {
                server_id,
                room_id,
                password,
            },
            agent_state,
            app,
        )
        .await,
    )
}

#[tauri::command]
pub async fn leave_room(app: AppHandle, state: State<'_, AppState>, server_id: String) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(handle_request(DaemonRequest::LeaveRoom { server_id }, agent_state, app).await)
}

#[tauri::command]
pub async fn delete_room(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
    room_id: String,
) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(handle_request(DaemonRequest::DeleteRoom { server_id, room_id }, agent_state, app).await)
}

#[tauri::command]
pub async fn set_room_in_progress(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(handle_request(DaemonRequest::SetRoomInProgress { server_id }, agent_state, app).await)
}

#[tauri::command]
pub async fn set_room_available(app: AppHandle, state: State<'_, AppState>, server_id: String) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(handle_request(DaemonRequest::SetRoomAvailable { server_id }, agent_state, app).await)
}

#[tauri::command]
pub async fn send_chat_message(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
    text: String,
) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(handle_request(DaemonRequest::SendChatMessage { server_id, text }, agent_state, app).await)
}

#[tauri::command]
pub async fn send_private_message(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
    to_client_id: String,
    text: String,
) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(
        handle_request(
            DaemonRequest::SendPrivateMessage {
                server_id,
                to_client_id,
                text,
            },
            agent_state,
            app,
        )
        .await,
    )
}

#[tauri::command]
pub async fn start_hosting_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    hub_host: String,
    hub_tcp_port: u16,
    room_token: String,
    game_port: u16,
    secure: bool,
) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(
        handle_request(
            DaemonRequest::StartHostingTunnel {
                hub_host,
                hub_tcp_port,
                room_token,
                game_port,
                secure,
            },
            agent_state,
            app,
        )
        .await,
    )
}

#[tauri::command]
pub async fn start_joining_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    hub_host: String,
    hub_tcp_port: u16,
    room_token: String,
    game_port: u16,
    secure: bool,
) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(
        handle_request(
            DaemonRequest::StartJoiningTunnel {
                hub_host,
                hub_tcp_port,
                room_token,
                game_port,
                secure,
            },
            agent_state,
            app,
        )
        .await,
    )
}

#[tauri::command]
pub async fn stop_hub_tunnel(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(handle_request(DaemonRequest::StopHubTunnel, agent_state, app).await)
}

#[cfg(test)]
#[path = "matchmaking_tests.rs"]
mod tests;
