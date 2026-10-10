use crate::core::error::AppError;
use crate::daemon_ipc::DaemonClient;
use crate::state::AppState;
use tauri::State;
use warlay::protocol::ipc::{AgentSettings, AgentStatusSnapshot, DaemonRequest, DaemonResponse};

/// Same pattern as `matchmaking.rs`'s own private `require_client` --
/// duplicated rather than shared, matching this codebase's existing
/// convention for this exact helper (see e.g. `daemon/src/logging.rs`'s
/// `log_dir` doc comment for the same reasoning applied elsewhere).
fn require_client(state: &State<'_, AppState>) -> Result<DaemonClient, AppError> {
    state
        .daemon_client
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::Any(anyhow::anyhow!("no hay conexión activa con el daemon")))
}

fn success_or_err(resp: DaemonResponse) -> Result<(), AppError> {
    match resp {
        DaemonResponse::Success { .. } => Ok(()),
        DaemonResponse::Error { message, .. } => Err(AppError::Any(anyhow::anyhow!(message))),
        other => Err(AppError::Any(anyhow::anyhow!(
            "respuesta inesperada del daemon: {other:?}"
        ))),
    }
}

#[tauri::command]
pub async fn get_agent_settings(state: State<'_, AppState>) -> Result<AgentSettings, AppError> {
    let client = require_client(&state)?;
    match client
        .call(DaemonRequest::GetAgentSettings)
        .await
        .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?
    {
        DaemonResponse::AgentSettings { settings } => Ok(settings),
        DaemonResponse::Error { message, .. } => Err(AppError::Any(anyhow::anyhow!(message))),
        other => Err(AppError::Any(anyhow::anyhow!(
            "respuesta inesperada del daemon: {other:?}"
        ))),
    }
}

#[tauri::command]
pub async fn save_agent_settings(state: State<'_, AppState>, settings: AgentSettings) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::SetAgentSettings { settings })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

#[tauri::command]
pub async fn get_agent_status(state: State<'_, AppState>) -> Result<AgentStatusSnapshot, AppError> {
    let client = require_client(&state)?;
    match client
        .call(DaemonRequest::GetAgentStatus)
        .await
        .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?
    {
        DaemonResponse::AgentStatus { status } => Ok(status),
        DaemonResponse::Error { message, .. } => Err(AppError::Any(anyhow::anyhow!(message))),
        other => Err(AppError::Any(anyhow::anyhow!(
            "respuesta inesperada del daemon: {other:?}"
        ))),
    }
}

#[tauri::command]
pub async fn retry_peer_mesh(state: State<'_, AppState>) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::RetryPeerMesh)
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}
