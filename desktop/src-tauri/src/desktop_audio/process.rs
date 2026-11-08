//! Which process tree to leave out of the shared audio.
//!
//! The other players' voices play inside Warlay's WebView2 (a child of
//! `warlay.exe`), and must never be sent back to them. But the GAME is also a
//! child of `warlay.exe` (Warlay launches it), and its sound is exactly what
//! should be shared -- so excluding `warlay.exe`'s whole tree would silence the
//! game. The exclusion therefore targets the WebView2 browser process: the
//! audio service that plays the voices is its descendant.

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

const WEBVIEW_EXE: &str = "msedgewebview2.exe";

/// The first direct child of `parent` whose executable is `exe_name`
/// (case-insensitive), if any.
pub fn child_process_named(parent: u32, exe_name: &str) -> Option<u32> {
    children_named(parent, exe_name).into_iter().next()
}

/// Every direct child of `parent` whose executable is `exe_name`
/// (case-insensitive).
pub fn children_named(parent: u32, exe_name: &str) -> Vec<u32> {
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return Vec::new();
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = Vec::new();
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            if entry.th32ParentProcessID == parent {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                if name.eq_ignore_ascii_case(exe_name) {
                    found.push(entry.th32ProcessID);
                }
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

/// The process whose tree must be excluded from the capture: Warlay's
/// WebView2 browser process. If it cannot be found, Warlay itself -- that
/// also silences the game, but it can never echo the voices back.
pub fn exclusion_pid() -> u32 {
    let own = std::process::id();
    match child_process_named(own, WEBVIEW_EXE) {
        Some(webview) => webview,
        None => {
            tracing::warn!("no se encontró el proceso de WebView2; se excluye todo Warlay, también el audio del juego");
            own
        }
    }
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
