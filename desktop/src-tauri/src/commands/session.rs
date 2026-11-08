//! Room sessions of the new architecture: the frontend gets the room's
//! session (helper + ticket) from the main server and hands it to the
//! runtime, which links the players directly.

use crate::core::error::AppError;
use crate::runtime::handlers::handle_request;
use crate::state::AppState;
use tauri::State;
use warlay::protocol::ipc::RuntimeRequest;

use super::runtime::{require_runtime_state, success_or_err};

#[tauri::command]
pub async fn start_room_session(
    state: State<'_, AppState>,
    room_id: String,
    user_id: String,
    is_host: bool,
    rendezvous_addr: String,
    relay_addr: String,
    ticket: String,
) -> Result<(), AppError> {
    let runtime_state = require_runtime_state(&state)?;
    success_or_err(
        handle_request(
            RuntimeRequest::StartRoomSession {
                room_id,
                user_id,
                is_host,
                rendezvous_addr,
                relay_addr,
                ticket,
            },
            runtime_state,
        )
        .await,
    )
}

#[tauri::command]
pub async fn stop_room_session(state: State<'_, AppState>) -> Result<(), AppError> {
    let runtime_state = require_runtime_state(&state)?;
    success_or_err(handle_request(RuntimeRequest::StopRoomSession, runtime_state).await)
}

#[tauri::command]
pub async fn start_game_tunnel(state: State<'_, AppState>, game_port: u16) -> Result<(), AppError> {
    let runtime_state = require_runtime_state(&state)?;
    success_or_err(handle_request(RuntimeRequest::StartGameTunnel { game_port }, runtime_state).await)
}

#[tauri::command]
pub async fn update_room_helper(
    state: State<'_, AppState>,
    rendezvous_addr: String,
    relay_addr: String,
) -> Result<(), AppError> {
    let runtime_state = require_runtime_state(&state)?;
    success_or_err(
        handle_request(
            RuntimeRequest::UpdateRoomHelper {
                rendezvous_addr,
                relay_addr,
            },
            runtime_state,
        )
        .await,
    )
}
