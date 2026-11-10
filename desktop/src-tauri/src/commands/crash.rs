//! The interface's side of crash reports (see `crate::crash`).

use crate::crash::{self, CrashSummary};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The reports waiting for the player, newest first.
#[tauri::command]
pub fn list_crash_reports() -> Vec<CrashSummary> {
    crash::list(&crash::dir())
}

/// The whole text of one report, to show or copy.
#[tauri::command]
pub fn read_crash_report(id: String) -> Option<String> {
    crash::read(&crash::dir(), &id)
}

#[tauri::command]
pub fn delete_crash_report(id: String) -> bool {
    crash::delete(&crash::dir(), &id)
}

/// Opens a new GitHub issue with the report filled in, for the player to read
/// and submit. Nothing is sent from here.
#[tauri::command]
pub fn report_crash_on_github(id: String) -> Result<(), String> {
    let report = crash::read(&crash::dir(), &id).ok_or("report_not_found")?;
    super::browser::open_url(&crash::issue_url(&report, VERSION))
}

/// Keeps an error the interface did not handle. Returns whether it was kept
/// (a limit stops a loop from filling the disk).
#[tauri::command]
pub fn record_ui_error(message: String, stack: String) -> bool {
    crash::write_ui_error(&message, &stack, VERSION)
}
