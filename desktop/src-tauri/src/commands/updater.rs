//! Update commands: look for a newer release on GitHub, and install it
//! (download, verify, silent install, restart) like Discord does.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::core::error::AppError;
use crate::state::AppState;
use crate::updater::{self, release, UpdateStatus};

/// What "Acerca de" shows about this installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: &'static str,
    pub version: String,
    /// Short commit this build was made from (`local` outside CI).
    pub build: String,
    /// When this build was made (Unix seconds): its release date.
    pub released_at: i64,
    /// When this version was installed or last updated (Unix seconds).
    pub installed_at: i64,
}

pub fn app_info(version: &str, build: &str, released_at: i64, installed_at: i64) -> AppInfo {
    AppInfo {
        name: "Warlay",
        version: version.to_string(),
        build: build.to_string(),
        released_at,
        installed_at,
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Version, build and dates of this installation. Works offline.
#[tauri::command]
pub fn get_app_info(app: AppHandle, state: State<'_, AppState>) -> Result<AppInfo, AppError> {
    let version = app.package_info().version.to_string();
    let installed_at = state.store.note_running_version(&version, now_unix())?;
    let released_at = env!("WARLAY_BUILD_UNIX").parse().unwrap_or(0);
    Ok(app_info(
        &version,
        env!("WARLAY_BUILD_COMMIT"),
        released_at,
        installed_at,
    ))
}

/// Download progress event, throttled to a few per second.
pub const PROGRESS_EVENT: &str = "update://progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    received_bytes: u64,
    total_bytes: u64,
}

/// Only one update runs at a time.
static INSTALLING: AtomicBool = AtomicBool::new(false);

fn current_exe() -> Result<std::path::PathBuf, AppError> {
    Ok(std::env::current_exe()?)
}

/// Asks GitHub for the latest release and compares it with this version.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<UpdateStatus, AppError> {
    let release = updater::latest_release().await.map_err(AppError::InvalidInput)?;
    let installed = updater::is_installed_copy(&current_exe()?);
    Ok(updater::status_for(
        &app.package_info().version.to_string(),
        &release,
        installed,
    ))
}

/// Downloads the latest release, verifies it, runs its installer and
/// restarts into the new version. The release is read again here: the UI's
/// earlier answer is not trusted.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), AppError> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err(AppError::InvalidInput("ya hay una actualización en curso".to_string()));
    }
    let result = install(&app).await;
    if result.is_err() {
        INSTALLING.store(false, Ordering::SeqCst);
    }
    result
}

async fn install(app: &AppHandle) -> Result<(), AppError> {
    let exe = current_exe()?;
    if !updater::is_installed_copy(&exe) {
        return Err(AppError::InvalidInput(
            "esta copia no fue instalada con el instalador; descarga la nueva versión desde GitHub".to_string(),
        ));
    }
    let release = updater::latest_release().await.map_err(AppError::InvalidInput)?;
    if !release::is_newer(&app.package_info().version.to_string(), &release.version) {
        return Err(AppError::InvalidInput("ya tienes la última versión".to_string()));
    }

    let mut last = Instant::now() - Duration::from_secs(1);
    let handle = app.clone();
    let installer = updater::download_installer(&release, move |received, total| {
        let done = total > 0 && received >= total;
        if done || last.elapsed() >= Duration::from_millis(150) {
            last = Instant::now();
            let _ = handle.emit(
                PROGRESS_EVENT,
                Progress {
                    received_bytes: received,
                    total_bytes: total,
                },
            );
        }
    })
    .await
    .map_err(AppError::InvalidInput)?;

    updater::spawn_installer_and_restart(&installer, &exe).map_err(AppError::InvalidInput)?;
    // Quit for real (hubs, mesh, tunnel and LAN capture close) so the
    // installer finds warlay.exe unlocked.
    app.exit(0);
    Ok(())
}

/// Opens the release page in the browser (for copies the installer cannot
/// update, like the portable one). Only this repository's releases open.
#[tauri::command]
pub fn open_release_page(url: String) -> Result<(), AppError> {
    if !url.starts_with(&format!("https://github.com/{}/releases", release::REPO)) {
        return Err(AppError::InvalidInput("dirección no permitida".to_string()));
    }
    super::browser::open_url(&url).map_err(AppError::InvalidInput)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_this_repositorys_release_pages_open() {
        assert!(open_release_page("https://evil.example/".to_string()).is_err());
        assert!(open_release_page("https://github.com/other/repo/releases".to_string()).is_err());
        assert!(open_release_page("file:///C:/Windows/System32/calc.exe".to_string()).is_err());
        assert!(open_release_page("javascript:alert(1)".to_string()).is_err());
    }

    #[test]
    fn app_info_carries_the_name_and_uses_the_keys_the_frontend_reads() {
        let json = serde_json::to_value(app_info("0.10.0", "abc1234", 100, 200)).unwrap();
        assert_eq!(json["name"], "Warlay");
        assert_eq!(json["version"], "0.10.0");
        assert_eq!(json["build"], "abc1234");
        assert_eq!(json["releasedAt"], 100);
        assert_eq!(json["installedAt"], 200);
    }

    #[test]
    fn the_build_is_stamped_by_the_build_script() {
        assert!(!env!("WARLAY_BUILD_COMMIT").is_empty());
        assert!(env!("WARLAY_BUILD_UNIX").parse::<i64>().unwrap() > 1_700_000_000);
    }

    #[test]
    fn progress_serialises_to_the_shape_the_frontend_reads() {
        let json = serde_json::to_value(Progress {
            received_bytes: 5,
            total_bytes: 10,
        })
        .unwrap();
        assert_eq!(json["receivedBytes"], 5);
        assert_eq!(json["totalBytes"], 10);
    }
}
