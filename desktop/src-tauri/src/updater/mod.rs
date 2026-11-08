//! Self-update from GitHub Releases, like Discord: look for a newer release,
//! download its installer, check the file against the release's
//! `SHA256SUMS.txt`, run it silently and start the new version.
//!
//! Only files published by this repository are ever downloaded
//! ([`release::is_trusted_url`]) and a file whose hash does not match is
//! deleted and never run.

pub mod release;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};

use release::{checksum_for, from_github_json, is_newer, ReleaseInfo, REPO};

/// The installer is ~40 MB; anything far beyond that is not ours.
const MAX_INSTALLER_BYTES: u64 = 300 * 1024 * 1024;

/// What the UI shows after looking for updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub notes: String,
    pub published_at: String,
    pub page_url: String,
    pub size_bytes: u64,
    /// This copy was put there by the installer, so the installer can replace
    /// it. A portable copy can only be updated by hand.
    pub can_self_update: bool,
}

/// Builds what the UI needs from a release and the running version.
pub fn status_for(current: &str, release: &ReleaseInfo, can_self_update: bool) -> UpdateStatus {
    let has_installer = release.installer.is_some() && release.checksums.is_some();
    UpdateStatus {
        current: current.to_string(),
        latest: release.version.clone(),
        available: is_newer(current, &release.version),
        notes: release.notes.clone(),
        published_at: release.published_at.clone(),
        page_url: release.page_url.clone(),
        size_bytes: release.installer.as_ref().map_or(0, |a| a.size),
        can_self_update: can_self_update && has_installer,
    }
}

/// Whether the running copy was installed (the NSIS installer leaves its
/// `uninstall.exe` next to it); a portable or development copy was not.
pub fn is_installed_copy(exe: &Path) -> bool {
    cfg!(windows) && exe.parent().is_some_and(|dir| dir.join("uninstall.exe").is_file())
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("warlay-desktop/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| format!("no se pudo preparar la conexión: {e}"))
}

/// The latest published release of this repository.
pub async fn latest_release() -> Result<ReleaseInfo, String> {
    let response = client()?
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("no se pudo consultar GitHub: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("GitHub respondió {}", response.status()));
    }
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("respuesta de GitHub no válida: {e}"))?;
    from_github_json(&json)
}

/// Downloads the release's installer into a temporary folder, reporting
/// `(received, total)` as it goes, and checks its SHA-256 against the
/// release's checksum list. The file is deleted if it does not match.
pub async fn download_installer(
    release: &ReleaseInfo,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    let installer = release
        .installer
        .as_ref()
        .ok_or("este release no incluye el instalador de Windows")?;
    let checksums = release
        .checksums
        .as_ref()
        .ok_or("este release no incluye SHA256SUMS.txt, no se puede verificar")?;
    let http = client()?;

    let listing = http
        .get(&checksums.url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("no se pudo leer SHA256SUMS.txt: {e}"))?
        .text()
        .await
        .map_err(|e| format!("SHA256SUMS.txt no se pudo leer: {e}"))?;
    let expected =
        checksum_for(&listing, &installer.name).ok_or_else(|| format!("SHA256SUMS.txt no lista {}", installer.name))?;

    let dir = std::env::temp_dir().join(format!("warlay-update-{}", release.version));
    std::fs::create_dir_all(&dir).map_err(|e| format!("no se pudo crear la carpeta temporal: {e}"))?;
    let path = dir.join("Warlay-setup.exe");

    let result = download_to(&http, &installer.url, &path, &expected, &mut on_progress).await;
    if result.is_err() {
        let _ = std::fs::remove_file(&path);
    }
    result.map(|()| path)
}

async fn download_to(
    http: &reqwest::Client,
    url: &str,
    path: &Path,
    expected_sha256: &str,
    on_progress: &mut impl FnMut(u64, u64),
) -> Result<(), String> {
    use std::io::Write;

    let mut response = http
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("no se pudo descargar la actualización: {e}"))?;
    let total = response.content_length().unwrap_or(0);
    if total > MAX_INSTALLER_BYTES {
        return Err("el instalador es demasiado grande, se descarta".to_string());
    }

    let mut file = std::fs::File::create(path).map_err(|e| format!("no se pudo guardar el instalador: {e}"))?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("se cortó la descarga: {e}"))?
    {
        received += chunk.len() as u64;
        if received > MAX_INSTALLER_BYTES {
            return Err("el instalador es demasiado grande, se descarta".to_string());
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .map_err(|e| format!("no se pudo guardar el instalador: {e}"))?;
        on_progress(received, total);
    }
    file.flush()
        .map_err(|e| format!("no se pudo guardar el instalador: {e}"))?;
    drop(file);

    verify_digest(&hex(&hasher.finalize()), expected_sha256)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The downloaded file must be exactly the one the release published.
pub fn verify_digest(actual: &str, expected: &str) -> Result<(), String> {
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err("la descarga no coincide con la suma de verificación del release; se descartó".to_string())
    }
}

/// The command line that runs the installer silently once this process has
/// gone, then starts the new version. Refuses paths `cmd` would reinterpret.
pub fn restart_command(installer: &Path, exe: &Path) -> Result<String, String> {
    for path in [installer, exe] {
        let text = path.to_string_lossy();
        if text
            .chars()
            .any(|c| matches!(c, '"' | '%' | '&' | '^' | '|' | '<' | '>' | '\n' | '\r'))
        {
            return Err(format!(
                "la ruta {text} tiene caracteres que no se pueden usar al actualizar"
            ));
        }
    }
    // The ping is a ~2 s pause (this process needs to be gone so the installer
    // can replace warlay.exe); `start /wait` blocks until the installer ends.
    Ok(format!(
        "ping -n 3 127.0.0.1 >nul & start /wait \"\" \"{}\" /S & start \"\" \"{}\"",
        installer.display(),
        exe.display()
    ))
}

/// Starts the detached installer-then-relaunch sequence. The caller must exit
/// right after, so the installer finds `warlay.exe` unlocked.
#[cfg(windows)]
pub fn spawn_installer_and_restart(installer: &Path, exe: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

    let script = restart_command(installer, exe)?;
    std::process::Command::new("cmd")
        .arg("/C")
        .raw_arg(script)
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("no se pudo iniciar el instalador: {e}"))
}

#[cfg(not(windows))]
pub fn spawn_installer_and_restart(_installer: &Path, _exe: &Path) -> Result<(), String> {
    Err("la actualización automática solo existe en Windows".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use release::Asset;

    fn info(version: &str, with_files: bool) -> ReleaseInfo {
        let asset = |name: &str, size| {
            with_files.then(|| Asset {
                name: name.to_string(),
                url: format!("https://github.com/{REPO}/releases/download/v{version}/{name}"),
                size,
            })
        };
        ReleaseInfo {
            version: version.to_string(),
            notes: "notas".to_string(),
            published_at: "2026-11-09T10:00:00Z".to_string(),
            page_url: release::releases_page(),
            installer: asset("warlay-desktop-vX-windows-x64-setup.exe", 42),
            checksums: asset("SHA256SUMS.txt", 1),
        }
    }

    #[test]
    fn a_newer_release_is_available_and_carries_what_the_ui_shows() {
        let s = status_for("0.9.0", &info("0.10.0", true), true);
        assert!(s.available);
        assert_eq!((s.current.as_str(), s.latest.as_str()), ("0.9.0", "0.10.0"));
        assert_eq!(s.size_bytes, 42);
        assert!(s.can_self_update);
    }

    #[test]
    fn the_same_or_an_older_release_is_not_an_update() {
        assert!(!status_for("0.10.0", &info("0.10.0", true), true).available);
        assert!(!status_for("0.10.0", &info("0.9.0", true), true).available);
    }

    #[test]
    fn a_portable_copy_or_a_release_without_files_cannot_self_update() {
        assert!(!status_for("0.9.0", &info("0.10.0", true), false).can_self_update);
        let s = status_for("0.9.0", &info("0.10.0", false), true);
        assert!(s.available);
        assert!(!s.can_self_update, "no installer or checksums to trust");
    }

    #[test]
    fn the_status_uses_the_camel_case_keys_the_frontend_reads() {
        let json = serde_json::to_value(status_for("0.9.0", &info("0.10.0", true), true)).unwrap();
        for key in [
            "current",
            "latest",
            "available",
            "notes",
            "publishedAt",
            "pageUrl",
            "sizeBytes",
            "canSelfUpdate",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
    }

    #[test]
    fn only_a_matching_digest_passes_whatever_its_case() {
        let h = "ab".repeat(32);
        assert!(verify_digest(&h, &h).is_ok());
        assert!(verify_digest(&h, &h.to_ascii_uppercase()).is_ok());
        assert!(verify_digest(&h, &"cd".repeat(32)).is_err());
        assert!(verify_digest("", &h).is_err());
    }

    #[test]
    fn hex_is_lowercase_and_padded() {
        assert_eq!(hex(&[0, 15, 255]), "000fff");
    }

    #[test]
    fn the_restart_command_runs_the_installer_silently_and_then_the_app() {
        let cmd = restart_command(
            Path::new(r"C:\Temp\Warlay-setup.exe"),
            Path::new(r"C:\Apps\Warlay\warlay.exe"),
        )
        .unwrap();
        let installer = cmd.find("Warlay-setup.exe\" /S").expect("silent install");
        let app = cmd.rfind("warlay.exe").expect("relaunch");
        assert!(installer < app, "install first, then start the app");
        assert!(cmd.contains("start /wait"), "waits for the installer");
    }

    #[test]
    fn paths_that_cmd_would_reinterpret_are_refused() {
        for bad in [
            r"C:\100%\a.exe",
            r"C:\a&b\a.exe",
            r"C:\a^b\a.exe",
            "C:\\a\"b\\a.exe",
            r"C:\a|b\a.exe",
        ] {
            assert!(
                restart_command(Path::new(bad), Path::new(r"C:\ok\warlay.exe")).is_err(),
                "{bad}"
            );
            assert!(
                restart_command(Path::new(r"C:\ok\s.exe"), Path::new(bad)).is_err(),
                "{bad}"
            );
        }
    }

    /// Serves `body` once on a local port, like a release asset would.
    async fn serve_once(body: Vec<u8>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request).await;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(&body).await.unwrap();
        });
        format!("http://{addr}/setup.exe")
    }

    #[tokio::test]
    async fn a_download_with_the_published_hash_is_kept_and_reports_progress() {
        let body = vec![7u8; 100_000];
        let expected = hex(&Sha256::digest(&body));
        let url = serve_once(body).await;
        let path = std::env::temp_dir().join("warlay-test-download-ok.exe");
        let mut last = (0, 0);

        download_to(&reqwest::Client::new(), &url, &path, &expected, &mut |r, t| {
            last = (r, t)
        })
        .await
        .unwrap();

        assert_eq!(std::fs::metadata(&path).unwrap().len(), 100_000);
        assert_eq!(last, (100_000, 100_000), "progress ends at the full size");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn a_download_with_another_hash_is_refused() {
        let url = serve_once(vec![7u8; 1_000]).await;
        let path = std::env::temp_dir().join("warlay-test-download-bad.exe");

        let result = download_to(&reqwest::Client::new(), &url, &path, &"00".repeat(32), &mut |_, _| {}).await;

        assert!(result.unwrap_err().contains("no coincide"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_folder_without_an_uninstaller_is_not_an_installed_copy() {
        assert!(!is_installed_copy(
            &std::env::temp_dir().join("warlay-not-installed.exe")
        ));
    }
}
