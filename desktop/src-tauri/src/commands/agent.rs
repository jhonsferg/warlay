use std::sync::Arc;
use tauri::async_runtime::Mutex as TokioMutex;

use anyhow::anyhow;
use tauri::{AppHandle, State};

use crate::agent::handlers::handle_request;
use crate::agent::state::DaemonState;
use crate::core::error::AppError;
use crate::state::AppState;
use warlay::protocol::ipc::{AgentSettings, AgentStatusSnapshot, DaemonRequest, DaemonResponse};

fn success_or_err(resp: DaemonResponse) -> Result<(), AppError> {
    match resp {
        DaemonResponse::Success { .. } => Ok(()),
        DaemonResponse::Error { message, .. } => Err(AppError::Any(anyhow!(message))),
        other => Err(AppError::Any(anyhow!("respuesta inesperada del agente: {other:?}"))),
    }
}

fn require_agent_state(state: &State<'_, AppState>) -> Result<Arc<TokioMutex<DaemonState>>, AppError> {
    state
        .agent_state
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::Any(anyhow!("el agente no está iniciado")))
}

#[tauri::command]
pub async fn get_agent_settings(state: State<'_, AppState>, app: AppHandle) -> Result<AgentSettings, AppError> {
    let agent_state = require_agent_state(&state)?;
    match handle_request(DaemonRequest::GetAgentSettings, agent_state, app).await {
        DaemonResponse::AgentSettings { settings } => Ok(settings),
        DaemonResponse::Error { message, .. } => Err(AppError::Any(anyhow!(message))),
        other => Err(AppError::Any(anyhow!("respuesta inesperada del agente: {other:?}"))),
    }
}

#[tauri::command]
pub async fn save_agent_settings(
    state: State<'_, AppState>,
    app: AppHandle,
    settings: AgentSettings,
) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(handle_request(DaemonRequest::SetAgentSettings { settings }, agent_state, app).await)
}

#[tauri::command]
pub async fn get_agent_status(state: State<'_, AppState>, app: AppHandle) -> Result<AgentStatusSnapshot, AppError> {
    let agent_state = require_agent_state(&state)?;
    match handle_request(DaemonRequest::GetAgentStatus, agent_state, app).await {
        DaemonResponse::AgentStatus { status } => Ok(status),
        DaemonResponse::Error { message, .. } => Err(AppError::Any(anyhow!(message))),
        other => Err(AppError::Any(anyhow!("respuesta inesperada del agente: {other:?}"))),
    }
}

#[tauri::command]
pub async fn retry_peer_mesh(state: State<'_, AppState>, app: AppHandle) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(handle_request(DaemonRequest::RetryPeerMesh, agent_state, app).await)
}
