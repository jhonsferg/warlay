use tauri::State;

use super::runtime::require_runtime_state;
use crate::core::error::AppError;
use crate::runtime::status::snapshot;
use crate::state::AppState;
use warlay::protocol::ipc::NetworkStatus;

/// The network state right now (hub connections, game tunnel, P2P links and
/// their ping). The UI reads it once on demand and then follows the
/// `network://status` event (see `runtime::status`).
#[tauri::command]
pub async fn get_network_status(state: State<'_, AppState>) -> Result<NetworkStatus, AppError> {
    let runtime = require_runtime_state(&state)?;
    let status = snapshot(&*runtime.lock().await);
    Ok(status)
}
