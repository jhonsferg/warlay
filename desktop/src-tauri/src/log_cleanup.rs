//! Deletes old per-launch log files (see `lib.rs`'s `log_file_name` -- one
//! file per process start, `warlay-YYYYMMDD-HHMMSS.log`) so
//! `%APPDATA%\com.warlay.lanlauncher\logs\` doesn't grow forever. Runs once
//! at startup, then every hour for as long as the app runs (it lives in the
//! tray for days, so "once per launch" left old files around), and again,
//! live, whenever the user changes the retention setting in Ajustes. The
//! same folder holds the daemon's logs, so this cleans those too.
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tauri::Manager;

/// How often the sweep repeats while the app is running.
const SWEEP_EVERY: Duration = Duration::from_secs(3600);

/// A file touched this recently is a log being written right now (this
/// process's, or the daemon's): never deleted, whatever the retention.
/// Windows lets an open file be deleted, which would silently orphan it.
const IN_USE_WINDOW: Duration = Duration::from_secs(120);

/// What the log folder holds.
#[derive(Serialize, Clone, Copy, Default, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LogUsage {
    pub files: u32,
    pub bytes: u64,
    /// Files removed by the sweep that produced this reading (0 if none ran).
    pub removed: u32,
}

/// Counts the regular files in `dir` and their total size.
pub fn usage(dir: &Path) -> LogUsage {
    let mut usage = LogUsage::default();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    usage.files += 1;
                    usage.bytes += meta.len();
                }
            }
        }
    }
    usage
}

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
    // 0 days means "everything not in use", not "everything".
    let max_age = Duration::from_secs(u64::from(retention_days) * 24 * 3600).max(IN_USE_WINDOW);
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

/// Sweeps once now and returns what is left. For the "Limpiar ahora" button.
pub async fn clean_now(dir: PathBuf, retention_days: u32) -> LogUsage {
    tauri::async_runtime::spawn_blocking(move || {
        let removed = clean_old_logs(&dir, retention_days) as u32;
        LogUsage { removed, ..usage(&dir) }
    })
    .await
    .unwrap_or_default()
}

/// Sweeps at startup and then every hour, reading the retention setting each
/// time so a change in Ajustes is picked up without a restart.
pub fn spawn_periodic(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let days = app
                .state::<crate::state::AppState>()
                .store
                .get_settings()
                .map(|settings| settings.log_retention_days)
                .unwrap_or(7);
            let dir = log_dir();
            let removed = tauri::async_runtime::spawn_blocking(move || clean_old_logs(&dir, days))
                .await
                .unwrap_or(0);
            if removed > 0 {
                tracing::info!(removed, retention_days = days, "logs antiguos eliminados");
            }
            tokio::time::sleep(SWEEP_EVERY).await;
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

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "warlay-log-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn age(path: &Path, by: Duration) {
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - by).unwrap();
    }

    #[test]
    fn a_log_being_written_is_never_removed_even_with_zero_days() {
        let dir = temp_dir("inuse");
        let live = dir.join("live.log");
        fs::write(&live, "being written right now").unwrap();

        assert_eq!(clean_old_logs(&dir, 0), 0);
        assert!(live.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn zero_days_removes_every_idle_file() {
        let dir = temp_dir("idle");
        let idle = dir.join("idle.log");
        fs::write(&idle, "from an earlier session").unwrap();
        age(&idle, Duration::from_secs(3600));

        assert_eq!(clean_old_logs(&dir, 0), 1);
        assert!(!idle.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn usage_counts_files_and_bytes() {
        let dir = temp_dir("usage");
        fs::write(dir.join("a.log"), vec![0_u8; 1000]).unwrap();
        fs::write(dir.join("b.log"), vec![0_u8; 234]).unwrap();
        fs::create_dir_all(dir.join("subdir")).unwrap();

        assert_eq!(
            usage(&dir),
            LogUsage {
                files: 2,
                bytes: 1234,
                removed: 0
            }
        );
        assert_eq!(usage(&dir.join("missing")), LogUsage::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_directory_is_a_no_op() {
        let dir = std::env::temp_dir().join("warlay-log-cleanup-test-does-not-exist");
        assert_eq!(clean_old_logs(&dir, 7), 0);
    }
}
