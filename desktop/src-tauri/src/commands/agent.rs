use std::sync::Arc;
use tauri::async_runtime::Mutex as TokioMutex;

use anyhow::anyhow;
use tauri::{AppHandle, State};

use crate::agent::handlers::handle_request;
use crate::agent::state::AgentState;
use crate::core::error::AppError;
use crate::state::AppState;
use warlay::protocol::ipc::{AgentSettings, NetworkStatus, AgentRequest, AgentResponse};

fn success_or_err(resp: AgentResponse) -> Result<(), AppError> {
    match resp {
        AgentResponse::Success { .. } => Ok(()),
        AgentResponse::Error { message, .. } => Err(AppError::Any(anyhow!(message))),
        other => Err(AppError::Any(anyhow!("respuesta inesperada del agente: {other:?}"))),
    }
}

fn require_agent_state(state: &State<'_, AppState>) -> Result<Arc<TokioMutex<AgentState>>, AppError> {
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
    match handle_request(AgentRequest::GetAgentSettings, agent_state, app).await {
        AgentResponse::AgentSettings { settings } => Ok(settings),
        AgentResponse::Error { message, .. } => Err(AppError::Any(anyhow!(message))),
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
    success_or_err(handle_request(AgentRequest::SetAgentSettings { settings }, agent_state, app).await)
}

/// What the agent is doing right now. The UI reads it once on demand and then
/// follows the `network://status` event (see `agent::status`).
#[tauri::command]
pub async fn get_network_status(state: State<'_, AppState>, app: AppHandle) -> Result<NetworkStatus, AppError> {
    let agent_state = require_agent_state(&state)?;
    match handle_request(AgentRequest::GetAgentStatus, agent_state, app).await {
        AgentResponse::AgentStatus { status } => Ok(status),
        AgentResponse::Error { message, .. } => Err(AppError::Any(anyhow!(message))),
        other => Err(AppError::Any(anyhow!("respuesta inesperada del agente: {other:?}"))),
    }
}

#[tauri::command]
pub async fn retry_peer_mesh(state: State<'_, AppState>, app: AppHandle) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(handle_request(AgentRequest::RetryPeerMesh, agent_state, app).await)
}

/// Temporary alias for the former command name.
#[tauri::command]
pub async fn get_agent_status(state: State<'_, AppState>, app: AppHandle) -> Result<NetworkStatus, AppError> {
    get_network_status(state, app).await
}
