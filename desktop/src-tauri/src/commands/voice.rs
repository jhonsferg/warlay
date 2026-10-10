use crate::core::error::AppError;
use crate::state::AppState;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn push_voice_frame(state: State<'_, AppState>, from_client_id: String, opus_bytes: Vec<u8>) {
    state.voice_manager.handle_voice_frame_in(from_client_id, opus_bytes);
}

#[tauri::command]
pub async fn start_voice_chat(
    state: State<'_, AppState>,
    room_token: String,
    input_device_id: Option<String>,
    output_device_id: Option<String>,
) -> Result<(), AppError> {
    let client = state
        .daemon_client
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::Any(anyhow::anyhow!("no hay conexión activa con el daemon")))?;
    state
        .voice_manager
        .start_voice(room_token, client, input_device_id, output_device_id)
        .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?;
    Ok(())
}

#[tauri::command]
pub fn stop_voice_chat(state: State<'_, AppState>) {
    state.voice_manager.stop_voice();
}

#[tauri::command]
pub fn toggle_mute_peer(state: State<'_, AppState>, app: AppHandle, client_id: String) {
    state.voice_manager.toggle_mute(&client_id);
    let _ = app.emit("voice://mute-changed", state.voice_manager.muted_peers());
}

#[tauri::command]
pub fn get_muted_peers(state: State<'_, AppState>) -> Vec<String> {
    state.voice_manager.muted_peers()
}

#[tauri::command]
pub fn toggle_self_mic_mute(state: State<'_, AppState>, app: AppHandle) -> bool {
    let current = state.voice_manager.is_self_mic_muted();
    let new_state = !current;
    state.voice_manager.set_self_mic_muted(new_state);
    let _ = app.emit("voice://self-mute-changed", new_state);
    new_state
}

#[tauri::command]
pub fn is_self_mic_muted(state: State<'_, AppState>) -> bool {
    state.voice_manager.is_self_mic_muted()
}
