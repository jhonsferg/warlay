//! Room sessions of the new architecture: the frontend gets the room's
//! session (helper + ticket) from the main server and hands it to the
//! agent, which links the players directly.

use crate::agent::handlers::handle_request;
use crate::core::error::AppError;
use crate::state::AppState;
use tauri::{AppHandle, State};
use warlay::protocol::ipc::AgentRequest;

use super::matchmaking::{require_agent_state, success_or_err};

#[tauri::command]
pub async fn start_room_session(
    app: AppHandle,
    state: State<'_, AppState>,
    room_id: String,
    user_id: String,
    is_host: bool,
    rendezvous_addr: String,
    relay_addr: String,
    ticket: String,
) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(
        handle_request(
            AgentRequest::StartRoomSession {
                room_id,
                user_id,
                is_host,
                rendezvous_addr,
                relay_addr,
                ticket,
            },
            agent_state,
            app,
        )
        .await,
    )
}

#[tauri::command]
pub async fn stop_room_session(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(handle_request(AgentRequest::StopRoomSession, agent_state, app).await)
}

#[tauri::command]
pub async fn start_game_tunnel(app: AppHandle, state: State<'_, AppState>, game_port: u16) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(handle_request(AgentRequest::StartGameTunnel { game_port }, agent_state, app).await)
}

#[tauri::command]
pub async fn update_room_helper(
    app: AppHandle,
    state: State<'_, AppState>,
    rendezvous_addr: String,
    relay_addr: String,
) -> Result<(), AppError> {
    let agent_state = require_agent_state(&state)?;
    success_or_err(
        handle_request(
            AgentRequest::UpdateRoomHelper {
                rendezvous_addr,
                relay_addr,
            },
            agent_state,
            app,
        )
        .await,
    )
}
