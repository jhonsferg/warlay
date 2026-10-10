use crate::core::dto::LaunchResultDto;
use crate::core::AppError;
use crate::launcher;
use crate::state::AppState;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::info;

/// Many real-world game installs (Battle.net-wrapped Warcraft III among
/// them) run the launched `.exe` only as a stub that spawns the actual game
/// process and exits within a second or two — `wait_for_exit` on *that*
/// child then resolves almost immediately, which used to fire
/// `game://exited` (and auto-leave the room) seconds after founding it,
/// while the real game was still opening. If the process we're watching
/// exits faster than this, we treat it as a hand-off, not a real quit, and
/// stay silent rather than report a false exit.
const MIN_REAL_SESSION: Duration = Duration::from_secs(5);

/// Launches the target game executable with the given arguments, detached
/// from the launcher process, and starts watching it for exit: when it
/// quits after having run for a while (however it quits — normal exit,
/// crash, killed externally), a `game://exited` event lets the UI show
/// "saliste del juego" — the room itself is untouched (the hub already
/// keeps a room alive across a `LeaveRoom`/disconnect; exiting WC3 doesn't
/// even send one), so the frontend can offer to relaunch and resume without
/// re-creating anything. An exit within `MIN_REAL_SESSION` of launch is
/// assumed to be a launcher stub handing off to the real process and is not
/// reported. Called again on every relaunch (including a "Reanudar
/// partida" resume), which naturally replaces the watcher below.
#[tauri::command]
pub async fn launch_game(
    app: AppHandle,
    state: State<'_, AppState>,
    executable_path: String,
    args: Vec<String>,
    working_dir: Option<String>,
) -> Result<LaunchResultDto, AppError> {
    let path = PathBuf::from(executable_path);
    let dir = working_dir.map(PathBuf::from);
    let launched = launcher::launch(&path, &args, dir.as_deref())?;
    let pid = launched.pid;

    // Replaces whatever job tracked the *previous* launch, if any --
    // explicitly closed here rather than left to Rust's `Drop` (`GameJob`
    // has none, deliberately: see `create_kill_on_close_job`'s doc comment)
    // so relaunching repeatedly within one Warlay session doesn't leak a
    // Job Object handle per relaunch. By the time a new launch happens the
    // previous one's game has essentially always already exited; on the
    // rare chance it hasn't, closing the last handle to a
    // `KILL_ON_JOB_CLOSE` job ends it immediately, which is the right
    // outcome for a launch that's about to be superseded anyway.
    #[cfg(windows)]
    {
        let mut guard = state.game_job.lock().expect("game job lock poisoned");
        if let Some(old) = guard.take() {
            launcher::close_job(old);
        }
        *guard = launched.job;
    }

    // Optimistic, immediate feedback for the status bar -- the background
    // poller (see `lib.rs`'s `.setup()`) would eventually catch this too,
    // but only on its next ~2s tick; no reason to make the UI wait that
    // long for a state change we already know happened. Written to the
    // shared `game_running` flag (not just emitted) so the poller's own
    // change-detection agrees with this and can still self-correct later
    // even if it never separately observed this specific transition itself
    // (e.g. this process dies again before the poller's next tick).
    state.game_running.store(true, Ordering::SeqCst);
    info!(
        pid,
        "estado de Warcraft III cambió: running=true (lanzado por esta app)"
    );
    let _ = app.emit("game://running-state-changed", true);
    crate::overlay::set_overlay_visible(&app, true);

    let app_for_exit = app.clone();
    let watcher = tokio::spawn(async move {
        let launched_at = Instant::now();
        launched.wait_for_exit().await;
        if launched_at.elapsed() < MIN_REAL_SESSION {
            info!(
                pid,
                elapsed_ms = launched_at.elapsed().as_millis() as u64,
                "salida rápida tratada como relevo de un stub de lanzador, no como salida real"
            );
            return;
        }
        app_for_exit
            .state::<AppState>()
            .game_running
            .store(false, Ordering::SeqCst);
        info!(
            pid,
            "estado de Warcraft III cambió: running=false (salida real detectada)"
        );
        let _ = app_for_exit.emit("game://running-state-changed", false);
        crate::overlay::set_overlay_visible(&app_for_exit, false);
        let _ = app_for_exit.emit("game://exited", pid);
    });

    let mut guard = state.game_exit_watcher.lock().expect("game exit watcher lock poisoned");
    if let Some(previous) = guard.take() {
        previous.abort();
    }
    *guard = Some(watcher);

    Ok(LaunchResultDto { pid })
}

/// Lets the UI validate a typed-in executable path in real time.
#[tauri::command]
pub fn path_exists(path: String) -> bool {
    std::path::Path::new(&path).is_file()
}

/// On-demand check for the status bar's initial state (seeded once at
/// startup, before the background poller's first tick -- see `lib.rs`'s
/// `.setup()`) and for anything else that needs a fresh answer rather than
/// waiting for the next push event.
#[tauri::command]
pub fn is_game_running(state: State<'_, AppState>) -> Result<bool, AppError> {
    #[cfg(windows)]
    {
        let job = *state.game_job.lock().expect("game job lock poisoned");
        if let Some(job) = job {
            return Ok(!launcher::job_process_ids(job).is_empty());
        }
    }
    let path = state.store.get_settings()?.game_executable_path;
    if path.trim().is_empty() {
        return Ok(false);
    }
    Ok(launcher::is_process_running_at(Path::new(&path)))
}
