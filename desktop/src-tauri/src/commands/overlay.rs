use crate::core::error::AppError;
use tauri::{AppHandle, Manager};

/// Toggles cursor pass-through for the overlay window.
/// When `interactive` is true, cursor events are captured by the overlay.
/// When `interactive` is false, cursor events pass through to underlying windows.
#[tauri::command]
pub fn set_overlay_interactive(app: AppHandle, interactive: bool) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window("overlay") {
        window
            .set_ignore_cursor_events(!interactive)
            .map_err(|e| AppError::Any(anyhow::anyhow!(e)))?;
    }
    Ok(())
}
