use crate::launcher;
use crate::state::AppState;
use std::path::Path;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Creates the overlay window dynamically at app launch.
/// The overlay starts hidden and click-through (`set_ignore_cursor_events(true)`).
pub fn setup_overlay(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
        .transparent(true)
        // `.transparent(true)` alone only allows alpha < 1.0 pixels to show
        // through -- it doesn't change what WebView2 paints by default. This
        // makes that default explicit (fully transparent) instead of relying
        // on every page ever loaded here to happen to paint alpha=0 everywhere
        // it needs to be see-through.
        .background_color(tauri::webview::Color(0, 0, 0, 0))
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        // Taller than before to fit the Google-Meet-style 48px circular
        // controls (push-to-talk indicator, mic toggle) below the header
        // without cramming the roster area.
        .inner_size(280.0, 175.0)
        .visible(false);

    // Fallback position for the brief window (pun intended) between the
    // overlay becoming visible and `update_overlay_position` finding the
    // real game window -- top-left, matching where it ends up once that
    // lookup succeeds (see `update_overlay_position`'s own doc comment).
    builder = builder.position(8.0, 124.0);

    let window = builder.build()?;
    let _ = window.set_ignore_cursor_events(true);

    Ok(())
}

/// Shows or hides the overlay window depending on whether the game is running.
pub fn set_overlay_visible(app: &AppHandle, visible: bool) {
    if let Some(window) = app.get_webview_window("overlay") {
        if visible {
            let _ = window.show();
            let path = app
                .state::<AppState>()
                .store
                .get_settings()
                .map(|s| s.game_executable_path)
                .unwrap_or_default();
            if !path.trim().is_empty() {
                update_overlay_position(app, Path::new(&path));
            }
        } else {
            let _ = window.hide();
        }
    }
}

/// Enumerates running processes to find process IDs matching the game executable's filename.
#[cfg(windows)]
pub fn get_game_pids(executable_path: &Path) -> Vec<u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };

    let Some(target_name) = executable_path.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };

    let snapshot = match unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) } {
        Ok(handle) => handle,
        Err(_) => return Vec::new(),
    };

    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };

    let mut pids = Vec::new();
    let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while has_entry {
        let name_end = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        let exe_name = String::from_utf16_lossy(&entry.szExeFile[..name_end]);
        if exe_name.eq_ignore_ascii_case(target_name) {
            pids.push(entry.th32ProcessID);
        }
        has_entry = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }

    unsafe {
        let _ = CloseHandle(snapshot);
    }
    pids
}

#[cfg(not(windows))]
pub fn get_game_pids(_executable_path: &Path) -> Vec<u32> {
    Vec::new()
}

/// Enumerates top-level visible windows belonging to the given PIDs and picks the one with the largest area.
#[cfg(windows)]
pub fn find_game_window(pids: &[u32]) -> Option<windows::Win32::Foundation::HWND> {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
    };

    struct CandidateWindow {
        hwnd: HWND,
        area: i64,
    }

    struct EnumWindowsState<'a> {
        pids: &'a [u32],
        best_candidate: Option<CandidateWindow>,
    }

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam.0 as *mut EnumWindowsState);
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));

        if state.pids.contains(&pid) {
            if IsWindowVisible(hwnd).as_bool() {
                let title_len = GetWindowTextLengthW(hwnd);
                if title_len > 0 {
                    let mut rect = RECT::default();
                    if GetWindowRect(hwnd, &mut rect).is_ok() {
                        let width = (rect.right - rect.left).max(0) as i64;
                        let height = (rect.bottom - rect.top).max(0) as i64;
                        let area = width * height;
                        if area > 0 {
                            match &mut state.best_candidate {
                                Some(best) => {
                                    if area > best.area {
                                        state.best_candidate = Some(CandidateWindow { hwnd, area });
                                    }
                                }
                                None => {
                                    state.best_candidate = Some(CandidateWindow { hwnd, area });
                                }
                            }
                        }
                    }
                }
            }
        }
        BOOL(1)
    }

    let mut state = EnumWindowsState {
        pids,
        best_candidate: None,
    };

    let lparam = LPARAM(&mut state as *mut _ as isize);
    let _ = unsafe { EnumWindows(Some(enum_windows_proc), lparam) };

    state.best_candidate.map(|c| c.hwnd)
}

/// Retrieves the on-screen rectangle of the window itself -- this is the
/// anchor `update_overlay_position` actually positions against (the game's
/// own bounds, not the monitor's), so the overlay sits at the corner of the
/// game window the way Xbox Game Bar's overlay does, instead of the corner
/// of the monitor regardless of where a windowed-mode game happens to be.
#[cfg(windows)]
pub fn get_window_rect(hwnd: windows::Win32::Foundation::HWND) -> Option<windows::Win32::Foundation::RECT> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok() {
        Some(rect)
    } else {
        None
    }
}

/// Retrieves the screen rectangle of the monitor containing the specified window.
#[cfg(windows)]
pub fn get_window_monitor_rect(hwnd: windows::Win32::Foundation::HWND) -> Option<windows::Win32::Foundation::RECT> {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};

    let hmonitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    let ok = unsafe { GetMonitorInfoW(hmonitor, &mut info) }.as_bool();
    if ok {
        Some(info.rcMonitor)
    } else {
        None
    }
}

/// Forces the overlay window back to the very top of the z-order via a
/// direct Win32 call -- deliberately not `WebviewWindow::set_always_on_top`,
/// which may silently no-op if Tauri's own state tracking thinks the flag
/// hasn't changed (it was already `true`). That matters here specifically
/// because this needs to be called repeatedly, every tick, to actually win
/// a z-order fight: the game can reclaim topmost for itself on any focus
/// change, so re-issuing the real SetWindowPos syscall every time -- not
/// just the one time the Tauri-level flag flips from false to true -- is
/// the whole point.
#[cfg(windows)]
pub fn reassert_overlay_topmost(app: &AppHandle) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};

    let Some(window) = app.get_webview_window("overlay") else {
        return;
    };
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

#[cfg(not(windows))]
pub fn reassert_overlay_topmost(_app: &AppHandle) {}

/// Repositions the overlay window to the top-left corner of the game
/// window itself -- not the monitor it's on (see `get_window_rect`'s doc
/// comment for why that distinction matters).
#[cfg(windows)]
pub fn update_overlay_position(app: &AppHandle, executable_path: &Path) {
    // Prefer the launch's own Job Object (see
    // `launcher::create_kill_on_close_job`) over name-matching -- it finds
    // the real game window even when a launcher stub handed off to a
    // differently-named process (e.g. a configured "Frozen Throne.exe"
    // running as "war3.exe"), which name-matching against the configured
    // path can never do. Falls back to name-matching only when there's no
    // job at all (game running outside Warlay's own launch button).
    let job = app
        .state::<AppState>()
        .game_job
        .lock()
        .expect("game job lock poisoned")
        .clone();
    let pids = match job {
        Some(job) => launcher::job_process_ids(job),
        None => get_game_pids(executable_path),
    };
    if pids.is_empty() {
        return;
    }

    let Some(hwnd) = find_game_window(&pids) else {
        return;
    };

    let Some(window_rect) = get_window_rect(hwnd) else {
        return;
    };

    // Only used for DPI scale and to clamp the overlay to visible screen
    // space -- the actual anchor is `window_rect` above.
    let Some(monitor_rect) = get_window_monitor_rect(hwnd) else {
        return;
    };

    let Some(window) = app.get_webview_window("overlay") else {
        return;
    };

    // Also reasserted here (in addition to the much faster dedicated loop
    // in `lib.rs`'s `.setup()`) so a position change and a topmost
    // reassertion always happen together at least once every ~2s tick.
    reassert_overlay_topmost(app);

    let scale_factor = app
        .available_monitors()
        .ok()
        .and_then(|monitors| {
            monitors.into_iter().find(|m| {
                let pos = m.position();
                pos.x == monitor_rect.left && pos.y == monitor_rect.top
            })
        })
        .map(|m| m.scale_factor())
        .unwrap_or_else(|| window.scale_factor().unwrap_or(1.0));

    // Top-left, flush against the game window's own left edge, well below
    // where a game's own top menu/HUD typically sits -- not the top-right
    // corner this used to sit in.
    let left_margin_logical = 8.0;
    let top_margin_logical = 124.0;
    let overlay_width_logical = 280.0;

    let left_margin_physical = (left_margin_logical * scale_factor).round() as i32;
    let top_margin_physical = (top_margin_logical * scale_factor).round() as i32;
    let overlay_width_physical = (overlay_width_logical * scale_factor).round() as i32;

    let raw_pos_x = window_rect.left + left_margin_physical;
    let raw_pos_y = window_rect.top + top_margin_physical;

    // Clamped to the monitor the game is actually on -- a maximized or
    // slightly off-screen game window can otherwise push the overlay a
    // few pixels past the visible edge.
    let pos_x_physical = raw_pos_x.clamp(monitor_rect.left, monitor_rect.right - overlay_width_physical);
    let pos_y_physical = raw_pos_y.clamp(monitor_rect.top, monitor_rect.bottom);

    let target_pos = tauri::Position::Physical(tauri::PhysicalPosition::new(pos_x_physical, pos_y_physical));

    if let Ok(current_pos) = window.outer_position() {
        if current_pos.x == pos_x_physical && current_pos.y == pos_y_physical {
            return;
        }
    }

    let _ = window.set_position(target_pos);
}

#[cfg(not(windows))]
pub fn update_overlay_position(_app: &AppHandle, _executable_path: &Path) {}
