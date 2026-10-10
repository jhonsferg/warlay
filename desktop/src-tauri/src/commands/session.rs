//! Room sessions of the new architecture: the frontend gets the room's
//! session (helper + ticket) from the main server and hands it to the
//! daemon, which links the players directly.

use crate::core::error::AppError;
use crate::state::AppState;
use tauri::State;
use warlay::protocol::ipc::DaemonRequest;

use super::matchmaking::{require_client, success_or_err};

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
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::StartRoomSession {
                room_id,
                user_id,
                is_host,
                rendezvous_addr,
                relay_addr,
                ticket,
            })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

#[tauri::command]
pub async fn stop_room_session(state: State<'_, AppState>) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::StopRoomSession)
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

#[tauri::command]
pub async fn start_game_tunnel(state: State<'_, AppState>, game_port: u16) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::StartGameTunnel { game_port })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}

/// The new frontend owns the room roster (it comes from the main server);
/// this keeps the in-game overlay's copy current and pushes it live.
#[tauri::command]
pub fn set_overlay_roster(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    room_id: Option<String>,
    members: Vec<warlay::protocol::RoomMember>,
) {
    use tauri::Emitter;
    let room = room_id.and_then(|id| uuid::Uuid::parse_str(&id).ok());
    *state.active_room.lock().unwrap() = room.map(|id| (id, members.clone()));
    let _ = app.emit("overlay://roster", &members);
}

#[tauri::command]
pub async fn update_room_helper(
    state: State<'_, AppState>,
    rendezvous_addr: String,
    relay_addr: String,
) -> Result<(), AppError> {
    let client = require_client(&state)?;
    success_or_err(
        client
            .call(DaemonRequest::UpdateRoomHelper {
                rendezvous_addr,
                relay_addr,
            })
            .await
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?,
    )
}
