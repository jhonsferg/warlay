use crate::core::error::AppError;
use crate::daemon_ipc::DaemonClient;
use crate::state::AppState;
use std::time::Duration;
use tauri::{AppHandle, State};
use warlay::protocol::ipc::DaemonRequest;

const PIPE_NAME: &str = r"\\.\pipe\warlay-daemon";

/// Pings whichever `DaemonClient` we already hold, if any, and reports
/// whether it actually answered -- a stale handle left over from a daemon
/// that crashed/was killed would otherwise look "connected" forever, since
/// nothing else routinely exercises it.
async fn ping_existing(state: &State<'_, AppState>) -> bool {
    let client = state.daemon_client.lock().unwrap().clone();
    let Some(client) = client else { return false };
    client.call(DaemonRequest::Ping).await.is_ok()
}

/// Tries to attach to a daemon that's already running (e.g. started by the
/// logon Scheduled Task before the desktop app even launched) without
/// spawning anything. On success, stores the connection for reuse.
async fn try_attach(app: &AppHandle, state: &State<'_, AppState>) -> bool {
    let Ok(client) = DaemonClient::connect(PIPE_NAME, app.clone()).await else {
        return false;
    };
    if client.call(DaemonRequest::Ping).await.is_err() {
        return false;
    }
    *state.daemon_client.lock().unwrap() = Some(client);
    true
}

/// Checked on startup, before showing the main UI: is a daemon already up
/// and answering, whether that's our own previously-stored connection or
/// one we haven't attached to yet.
#[tauri::command]
pub async fn check_daemon_ready(app: AppHandle, state: State<'_, AppState>) -> Result<bool, AppError> {
    if ping_existing(&state).await {
        return Ok(true);
    }
    Ok(try_attach(&app, &state).await)
}

/// Launches `wnetd.exe` requesting Windows' elevation prompt (the `runas`
/// shell verb -- the same mechanism as right-click "Run as administrator"),
/// rather than the silent, non-elevated spawn this used to do. A daemon
/// spawned without elevation starts, answers pings, and looks perfectly
/// healthy right up until the first LAN capture attempt, which fails with
/// no visible symptom until then -- worth the one UAC prompt to avoid.
#[cfg(windows)]
fn spawn_daemon_elevated(daemon_path: &std::path::Path) -> windows::core::Result<()> {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let file = HSTRING::from(daemon_path.as_os_str());
    let verb = HSTRING::from("runas");

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC,
        lpVerb: windows::core::PCWSTR(verb.as_ptr()),
        lpFile: windows::core::PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };

    unsafe { ShellExecuteExW(&mut info) }
}

/// Confirms a daemon is up (attaching to one already running when possible),
/// and if none is reachable, launches one elevated and waits for it to
/// start answering. This is what the desktop app's startup loading screen
/// calls before showing the rest of the UI.
#[tauri::command]
pub async fn ensure_daemon_running(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    if ping_existing(&state).await || try_attach(&app, &state).await {
        return Ok(());
    }

    let exe_path = std::env::current_exe()
        .map_err(|e| AppError::Any(anyhow::anyhow!("no se pudo resolver la ruta de este ejecutable: {e}")))?;
    let daemon_path = exe_path.with_file_name("wnetd.exe");

    // ShellExecuteExW with SEE_MASK_NOASYNC blocks the calling thread until
    // the UAC prompt is resolved (however long the user takes to click
    // Yes/No) -- calling it directly on a Tokio worker thread would stall
    // the whole async runtime for that entire wait, including the WebView2
    // IPC that keeps the loading screen itself responsive. spawn_blocking
    // moves it to a dedicated thread instead.
    tokio::task::spawn_blocking(move || spawn_daemon_elevated(&daemon_path))
        .await
        .map_err(|e| AppError::Any(anyhow::anyhow!("el hilo de elevación falló: {e}")))?
        .map_err(|e| {
            AppError::Any(anyhow::anyhow!(
                "no se pudo iniciar el agente con privilegios elevados: {e}"
            ))
        })?;

    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(300)).await;
        if try_attach(&app, &state).await {
            return Ok(());
        }
    }

    Err(AppError::Any(anyhow::anyhow!(
        "el agente no respondió después de iniciarlo"
    )))
}
