//! Where the daemon keeps its database and logs.
//!
//! Windows: `%APPDATA%\com.warlay.lanlauncher` -- the same directory the
//! desktop app uses, hardcoded independently since the daemon doesn't
//! depend on desktop's crate (and so the app's log retention sweep also
//! cleans the daemon's logs). Linux: `$WARLAY_DATA_DIR`, else
//! `$XDG_DATA_HOME/warlay`, else `~/.local/share/warlay`.

use std::path::PathBuf;

#[cfg(windows)]
pub fn data_dir() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata).join("com.warlay.lanlauncher")
}

#[cfg(not(windows))]
pub fn data_dir() -> PathBuf {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = var("WARLAY_DATA_DIR") {
        return dir;
    }
    if let Some(dir) = var("XDG_DATA_HOME") {
        return dir.join("warlay");
    }
    match var("HOME") {
        Some(home) => home.join(".local").join("share").join("warlay"),
        None => PathBuf::from(".warlay"),
    }
}
