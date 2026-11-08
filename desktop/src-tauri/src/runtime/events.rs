use std::sync::OnceLock;

use tauri::{AppHandle, Emitter};

use warlay::protocol::ipc::RuntimeResponse;

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

pub fn set_app_handle(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
}

pub fn app_handle() -> Option<&'static AppHandle> {
    APP_HANDLE.get()
}

/// Emits a state-broadcast `RuntimeResponse` as a Tauri event to the frontend.
pub fn emit_runtime_event(app: &AppHandle, response: &RuntimeResponse) {
    match response {
        RuntimeResponse::GameStarted => {
            let _ = app.emit("session://game-started", ());
        }
        RuntimeResponse::GameEnded => {
            let _ = app.emit("session://game-ended", ());
        }
        RuntimeResponse::TunnelStatusChanged { running } => {
            let _ = app.emit("hubTunnel://status-changed", *running);
        }
        _ => {
            // Success / Error replies are answers, not broadcasts: nothing
            // to route them to.
        }
    }
}

/// Shorthand for a `RuntimeResponse::Error` reply.
pub fn err(request_type: &str, message: impl std::fmt::Display) -> RuntimeResponse {
    RuntimeResponse::Error {
        request_type: request_type.to_string(),
        message: message.to_string(),
    }
}

/// Emite un evento de estado al frontend, obteniendo el `AppHandle` del
/// `OnceLock` rellenado al arrancar.
pub fn emit_event(response: &RuntimeResponse) {
    if let Some(app) = APP_HANDLE.get() {
        emit_runtime_event(app, response);
    }
}
