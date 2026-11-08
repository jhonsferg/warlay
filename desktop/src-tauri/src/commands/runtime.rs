//! Helpers the commands that drive the network runtime share.

use std::sync::Arc;

use anyhow::anyhow;
use tauri::async_runtime::Mutex as TokioMutex;
use tauri::State;

use crate::core::error::AppError;
use crate::runtime::state::RuntimeState;
use crate::state::AppState;
use warlay::protocol::ipc::RuntimeResponse;

/// Every command that drives the runtime needs it to be running; silently
/// no-op'ing when it isn't hides real failures from the user instead of
/// surfacing them.
pub(crate) fn require_runtime_state(state: &State<'_, AppState>) -> Result<Arc<TokioMutex<RuntimeState>>, AppError> {
    state
        .runtime_state
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::Any(anyhow!("la red de Warlay aún no está iniciada")))
}

/// Maps the runtime's generic Success/Error reply to a Tauri command result.
pub(crate) fn success_or_err(resp: RuntimeResponse) -> Result<(), AppError> {
    match resp {
        RuntimeResponse::Success { .. } => Ok(()),
        RuntimeResponse::Error { message, .. } => Err(AppError::Any(anyhow!(message))),
        other => Err(AppError::Any(anyhow!(
            "respuesta inesperada de la red de Warlay: {other:?}"
        ))),
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
