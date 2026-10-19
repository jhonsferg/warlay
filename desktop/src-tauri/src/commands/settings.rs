use crate::core::dto::AppSettings;
use crate::core::AppError;
use crate::log_cleanup;
use crate::log_control::{self, LogTarget};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn load_settings(state: State<'_, AppState>) -> Result<AppSettings, AppError> {
    Ok(state.store.get_settings()?)
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, state: State<'_, AppState>, settings: AppSettings) -> Result<(), AppError> {
    let previous = state.store.get_settings().ok();
    let old_mic_toggle_key = previous.map(|s| s.mic_toggle_key);
    state.store.save_settings(&settings)?;
    *state
        .mic_toggle_shortcut
        .lock()
        .expect("mic toggle shortcut lock poisoned") =
        crate::voice::register_shortcut(&app, old_mic_toggle_key.as_deref(), &settings.mic_toggle_key);
    Ok(())
}

/// Applies a new log level immediately (see `log_control`) and persists it
/// so it survives a restart — the one settings control meant to feel
/// instant rather than needing a "Guardar" click.
#[tauri::command]
pub fn set_log_level(state: State<'_, AppState>, target: LogTarget, level: String) -> Result<(), AppError> {
    log_control::set_level(target, &level).map_err(AppError::InvalidInput)?;
    match target {
        LogTarget::Console => state.store.set_console_log_level(&level)?,
        LogTarget::File => state.store.set_file_log_level(&level)?,
    }
    Ok(())
}

/// How much the log folder (the app's and the daemon's) holds right now.
#[tauri::command]
pub fn get_log_usage() -> log_cleanup::LogUsage {
    log_cleanup::usage(&log_cleanup::log_dir())
}

/// Sweeps the log folder right now: files older than the retention window, or
/// with `all_inactive` every file that isn't being written to. Returns what
/// is left and how many files went.
#[tauri::command]
pub async fn clean_logs_now(state: State<'_, AppState>, all_inactive: bool) -> Result<log_cleanup::LogUsage, AppError> {
    let days = if all_inactive {
        0
    } else {
        state.store.get_settings()?.log_retention_days
    };
    Ok(log_cleanup::clean_now(log_cleanup::log_dir(), days).await)
}

/// Persists the new retention window and immediately re-runs the cleanup
/// sweep with it — a shorter value should visibly delete old logs right
/// away rather than only taking effect on the next launch.
#[tauri::command]
pub fn set_log_retention_days(state: State<'_, AppState>, days: u32) -> Result<(), AppError> {
    state.store.set_log_retention_days(days)?;
    log_cleanup::spawn_cleanup(log_cleanup::log_dir(), days);
    Ok(())
}
