//! Deletes old per-launch log files (see `lib.rs`'s `log_file_name` -- one
//! file per process start, `warlay-YYYYMMDD-HHMMSS.log`) so
//! `%APPDATA%\com.warlay.lanlauncher\logs\` doesn't grow forever. Runs once
//! at startup and again, live, whenever the user changes the retention
//! setting in Ajustes -- there's no periodic timer, since "once per app
//! launch" is already the natural cadence a desktop app gets for this kind
//! of housekeeping, and a change made mid-session should be felt immediately
//! rather than waiting for the next restart.
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// This is a windowed (GUI) app, not a console one — launched by
/// double-click, there is no terminal for `tracing`'s default stdout writer
/// to reach, so logs always land in a file here too (see `lib.rs`), in
/// addition to stdout for whoever *does* run it from a terminal.
///
/// Same AppData-path-without-an-app-handle trick as `lib.rs`'s
/// `client_db_path` — this needs to run before `.setup()`'s `AppHandle`
/// exists, and is also called later from `commands::settings` where only
/// `Tauri::State` (not the handle) is in scope.
pub fn log_dir() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    Path::new(&appdata).join("com.warlay.lanlauncher").join("logs")
}

/// Deletes every regular file directly inside `dir` last modified more than
/// `retention_days` ago. Best-effort per file: a file that can't be removed
/// (e.g. the current session's own log, still open for writing -- a real
/// lock on Windows) is skipped rather than aborting the whole sweep. Returns
/// how many files were actually removed.
fn clean_old_logs(dir: &Path, retention_days: u32) -> usize {
    let max_age = Duration::from_secs(u64::from(retention_days) * 24 * 3600);
    let Some(cutoff) = SystemTime::now().checked_sub(max_age) else {
        return 0;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };

    let mut removed = 0;
    for entry in entries.flatten() {
        let is_file = entry.file_type().map(|t| t.is_file()).unwrap_or(false);
        if !is_file {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if modified < cutoff && std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Runs the sweep off the async runtime's blocking pool (filesystem I/O) so
/// nothing waits on it -- fire-and-forget from both `lib.rs`'s startup path
/// and `commands::settings::set_log_retention_days`.
pub fn spawn_cleanup(dir: PathBuf, retention_days: u32) {
    tauri::async_runtime::spawn(async move {
        let removed = tauri::async_runtime::spawn_blocking(move || clean_old_logs(&dir, retention_days))
            .await
            .unwrap_or(0);
        if removed > 0 {
            tracing::info!(removed, retention_days, "logs antiguos eliminados");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn removes_only_files_older_than_retention() {
        let dir = std::env::temp_dir().join(format!("warlay-log-cleanup-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let old = dir.join("old.log");
        let fresh = dir.join("fresh.log");
        fs::write(&old, "old").unwrap();
        fs::write(&fresh, "fresh").unwrap();

        // `File::open` is read-only, which Windows won't allow SetFileTime
        // on — needs a handle opened for write.
        let ten_days_ago = SystemTime::now() - Duration::from_secs(10 * 24 * 3600);
        let file = fs::OpenOptions::new().write(true).open(&old).unwrap();
        file.set_modified(ten_days_ago).unwrap();

        let removed = clean_old_logs(&dir, 7);

        assert_eq!(removed, 1);
        assert!(!old.exists());
        assert!(fresh.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_directory_is_a_no_op() {
        let dir = std::env::temp_dir().join("warlay-log-cleanup-test-does-not-exist");
        assert_eq!(clean_old_logs(&dir, 7), 0);
    }
}
