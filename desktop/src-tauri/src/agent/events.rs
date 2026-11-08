use std::sync::OnceLock;

use tauri::{AppHandle, Emitter};

use crate::core::dto::{ServerMessageEvent, ServerPingEvent, ServerStatusEvent};
use warlay::protocol::ipc::AgentResponse;

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

pub fn set_app_handle(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
}

pub fn app_handle() -> Option<&'static AppHandle> {
    APP_HANDLE.get()
}

/// Emits a state-broadcast `AgentResponse` as a Tauri event to the frontend,
/// replacing the old `ipc_server::push_event` that wrote to a named pipe.
///
/// Runs inside the same process as the GUI now, so there is no pipe to
/// write to -- instead we emit the very same events directly as Tauri
/// events to the frontend.
pub fn emit_agent_event(app: &AppHandle, response: &AgentResponse) {
    match response {
        AgentResponse::HubConnectionState { server_id, connected } => {
            let _ = app.emit(
                "matchmaking://status",
                &ServerStatusEvent {
                    server_id: server_id.clone(),
                    connected: *connected,
                },
            );
        }
        AgentResponse::HubPing { server_id, latency_ms } => {
            let _ = app.emit(
                "matchmaking://ping",
                &ServerPingEvent {
                    server_id: server_id.clone(),
                    ping_ms: *latency_ms as u64,
                },
            );
        }
        AgentResponse::HubMessage { server_id, message } => {
            let _ = app.emit(
                "matchmaking://message",
                &ServerMessageEvent {
                    server_id: server_id.clone(),
                    message: message.clone(),
                },
            );
        }
        AgentResponse::GameStarted => {
            let _ = app.emit("session://game-started", ());
        }
        AgentResponse::GameEnded => {
            let _ = app.emit("session://game-ended", ());
        }
        AgentResponse::TunnelStatusChanged { running } => {
            let _ = app.emit("hubTunnel://status-changed", *running);
        }
        _ => {
            // Success / Error / Pong / Connected / ConnectFailed with no
            // (or a since-timed-out) matching pending call -- nothing left
            // to route it to.
        }
    }
}

/// Shorthand for a `AgentResponse::Error` reply.
pub fn err(request_type: &str, message: impl std::fmt::Display) -> AgentResponse {
    AgentResponse::Error {
        request_type: request_type.to_string(),
        message: message.to_string(),
    }
}

/// Emite un evento de estado del agente al frontend, obteniendo el
/// `AppHandle` del `OnceLock` rellenado al arrancar. Sustituye al viejo
/// `ipc_server::push_event` que escribía en el pipe.
pub fn emit_event(response: &AgentResponse) {
    if let Some(app) = APP_HANDLE.get() {
        emit_agent_event(app, response);
    }
}
