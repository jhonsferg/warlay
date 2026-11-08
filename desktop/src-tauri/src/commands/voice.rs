//! Voice transport for the in-room voice chat: the webview hands each encoded
//! Opus frame here as a raw binary body and it is sent to every connected
//! member over the mesh (unreliable datagrams, like real-time audio wants).

use std::str::FromStr;

use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::core::error::AppError;
use crate::state::AppState;

/// Largest frame accepted: a QUIC datagram must stay under the path MTU, and
/// a 20 ms Opus frame is ~100 bytes, so anything near this is a bug.
const MAX_FRAME_BYTES: usize = 1000;

/// Validates the invoke body: it must be raw bytes, non-empty and small.
fn frame_from_body(body: &InvokeBody) -> Result<Vec<u8>, AppError> {
    let InvokeBody::Raw(bytes) = body else {
        return Err(AppError::InvalidInput(
            "el frame de voz debe enviarse como bytes crudos".to_string(),
        ));
    };
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(AppError::InvalidInput(format!(
            "frame de voz de {} bytes fuera de rango (1..={MAX_FRAME_BYTES})",
            bytes.len()
        )));
    }
    Ok(bytes.clone())
}

/// Sends one encoded voice frame to the room. Returns how many members it
/// reached (0 when alone or not linked yet -- not an error).
#[tauri::command]
pub async fn send_voice_frame(request: Request<'_>, state: State<'_, AppState>) -> Result<usize, AppError> {
    let frame = frame_from_body(request.body())?;
    let agent_state = state.agent_state.lock().unwrap().clone();
    let Some(agent_state) = agent_state else {
        return Ok(0);
    };
    let mesh = agent_state.lock().await.peer_mesh.clone();
    Ok(mesh.map(|mesh| mesh.broadcast_voice_frame(&frame)).unwrap_or(0))
}

/// Starts sharing the PC's sound with the room (everything but Warlay's own
/// audio). Needs an active room session; idempotent while already sharing.
#[tauri::command]
pub async fn start_desktop_audio(state: State<'_, AppState>) -> Result<(), AppError> {
    if state.desktop_audio.lock().unwrap().is_some() {
        return Ok(());
    }
    let agent_state = state.agent_state.lock().unwrap().clone();
    let mesh = match agent_state {
        Some(agent_state) => agent_state.lock().await.peer_mesh.clone(),
        None => None,
    };
    let mesh = mesh.ok_or_else(|| AppError::InvalidInput("no hay una sala activa para compartir el audio".to_string()))?;
    // Opening WASAPI blocks for a moment: keep it off the async runtime.
    let share = tauri::async_runtime::spawn_blocking(move || crate::desktop_audio::start(mesh))
        .await
        .map_err(|e| AppError::InvalidInput(format!("la captura no pudo iniciar: {e}")))?
        .map_err(AppError::InvalidInput)?;
    *state.desktop_audio.lock().unwrap() = Some(share);
    Ok(())
}

/// Stops sharing the PC's sound. Safe to call when nothing is shared.
#[tauri::command]
pub fn stop_desktop_audio(state: State<'_, AppState>) {
    // Dropping the share stops the capture thread.
    state.desktop_audio.lock().unwrap().take();
}

/// Remembers (or withdraws) the user's answer to Warlay's own microphone
/// dialog. Must be set BEFORE the webview calls `getUserMedia`: the
/// permission policy (`permissions::decide`) reads it.
#[tauri::command]
pub fn set_mic_consent(state: State<'_, AppState>, granted: bool) -> Result<(), AppError> {
    state.store.set_mic_consent(granted)?;
    Ok(())
}

/// Opens Windows' "Microphone" privacy page, for when the OS itself blocks
/// desktop apps from the microphone (nothing Warlay can override).
#[tauri::command]
pub fn open_mic_privacy_settings() -> Result<(), AppError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", "ms-settings:privacy-microphone"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()?;
    }
    Ok(())
}

/// Default global mute toggle, usable while the game has focus.
pub const DEFAULT_MUTE_SHORTCUT: &str = "Ctrl+Shift+M";

fn parse_accelerator(accelerator: &str) -> Result<Shortcut, AppError> {
    Shortcut::from_str(accelerator.trim())
        .map_err(|e| AppError::InvalidInput(format!("atajo '{accelerator}' no válido: {e}")))
}

/// Registers the global mute shortcut (replacing a previous one). Only
/// registered while the user is in voice, so the key isn't hijacked the rest
/// of the time. Returns the accelerator in effect.
#[tauri::command]
pub fn register_voice_shortcut(
    app: AppHandle,
    state: State<'_, AppState>,
    accelerator: Option<String>,
) -> Result<String, AppError> {
    let accelerator = accelerator.unwrap_or_else(|| DEFAULT_MUTE_SHORTCUT.to_string());
    let shortcut = parse_accelerator(&accelerator)?;
    let mut current = state.voice_shortcut.lock().unwrap();
    if let Some(previous) = current.take() {
        let _ = app.global_shortcut().unregister(previous);
    }
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| AppError::InvalidInput(format!("no se pudo registrar '{accelerator}': {e}")))?;
    *current = Some(shortcut);
    Ok(accelerator)
}

#[tauri::command]
pub fn unregister_voice_shortcut(app: AppHandle, state: State<'_, AppState>) {
    if let Some(shortcut) = state.voice_shortcut.lock().unwrap().take() {
        let _ = app.global_shortcut().unregister(shortcut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_shortcut_and_common_accelerators_parse() {
        assert!(parse_accelerator(DEFAULT_MUTE_SHORTCUT).is_ok());
        assert!(parse_accelerator("  Alt+M ").is_ok());
        assert!(parse_accelerator("F9").is_ok());
    }

    #[test]
    fn garbage_accelerators_are_rejected_with_a_readable_error() {
        for bad in ["", "Ctrl+", "Ctrl+Shift+NoSuchKey", "+++"] {
            let err = parse_accelerator(bad).unwrap_err();
            assert!(matches!(err, AppError::InvalidInput(_)), "{bad:?}");
        }
    }

    #[test]
    fn only_small_non_empty_raw_bodies_are_accepted() {
        assert_eq!(frame_from_body(&InvokeBody::Raw(vec![1, 2, 3])).unwrap(), vec![1, 2, 3]);
        assert!(frame_from_body(&InvokeBody::Raw(vec![0; MAX_FRAME_BYTES])).is_ok());

        for bad in [vec![], vec![0; MAX_FRAME_BYTES + 1]] {
            assert!(matches!(
                frame_from_body(&InvokeBody::Raw(bad)),
                Err(AppError::InvalidInput(_))
            ));
        }
        assert!(matches!(
            frame_from_body(&InvokeBody::Json(serde_json::json!([1, 2, 3]))),
            Err(AppError::InvalidInput(_))
        ));
    }
}
