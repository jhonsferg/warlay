mod commands;
mod core;
pub use warlay::crypto;
mod daemon_ipc;
mod launcher;
mod log_bridge;
mod log_cleanup;
mod log_control;
mod network;
mod overlay;
pub use warlay::protocol;
mod state;
mod storage;
pub use warlay::transport;
pub mod voice;

use state::AppState;
use storage::ClientStore;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use warlay::protocol::ipc::DaemonRequest;

/// Same AppData-path-without-an-app-handle trick as `log_cleanup::log_dir` — the
/// local database needs to be open *before* the tracing subscriber is built
/// (its stored log levels seed the initial filters), which is earlier than
/// `.setup()`'s `AppHandle` exists.
fn client_db_path() -> std::path::PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    std::path::Path::new(&appdata)
        .join("com.warlay.lanlauncher")
        .join("warlay.db")
}

/// `std::fs::File`'s `Write::flush` is a documented no-op on every
/// platform, and `RollingFileAppender` (previously used here) just
/// delegates its own `flush` to that same no-op inner file. Fine for
/// getting bytes into the OS, but on Windows another process reading this
/// file live (tailing the log while the app is still running) sees a
/// stale/lagging size for as long as this handle stays open -- NTFS
/// updates the visible end-of-file for *other* openers lazily, not on
/// every write. `tracing_appender`'s worker calls `flush()` after every
/// batch of lines it drains, so routing that through `sync_data()`
/// (Windows' `FlushFileBuffers`) instead keeps the on-disk size honest for
/// live tailing, not just after this process's handle finally closes.
/// Same fix, same reasoning, as `daemon/src/logging.rs`'s equivalent.
struct SyncOnFlush(std::fs::File);

impl std::io::Write for SyncOnFlush {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.sync_data()
    }
}

/// One file per process launch, named for when it started — e.g.
/// `warlay-20260727-084413.log` — instead of every session appending to a
/// single ever-growing `warlay.log`. Comparing a host's and a joiner's run
/// side by side (the whole point of this log, when diagnosing why a room
/// isn't working) is only practical if each launch is its own file: with
/// one shared file, isolating "just this test" means manually finding
/// where in a possibly-huge file this run's lines start.
fn log_file_name() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "warlay-{:04}{:02}{:02}-{:02}{:02}{:02}.log",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// Registers a Windows Firewall inbound-allow rule for this exact exe, on
/// every network profile (including "Public" — which is what Windows
/// classifies most cellular/mobile-hotspot connections as by default). This
/// matters specifically for `start_joining`'s local TCP listener on
/// `game_port`: when a real WC3 announcement is rebroadcast on the joiner's
/// own network, WC3 ends up trying to connect *back to one of this
/// machine's own local IPs* to join (there's no host address embedded in
/// `W3GS_GAMEINFO`, only a name/port — the network layer's source address
/// is literally the only address WC3 has to go on). On a Public-profile
/// adapter, Windows Firewall silently drops that inbound connection before
/// it ever reaches our listener — no error, no log, it just never arrives,
/// which is exactly what a real test showed. Re-registering on every launch
/// (delete then add) keeps this idempotent rather than accumulating
/// duplicate rules across runs. Needs the same Administrator elevation
/// `lan_capture` already requires, so it costs nothing new to ask for;
/// failing here is non-fatal — hosting/joining still work, this only
/// affects whether a *joiner on a Public-profile network* can actually
/// receive the inbound game connection.
/// A Windows `CREATE_NO_WINDOW` process flag — without it, spawning a
/// console-subsystem child (like `netsh.exe`) from this GUI-subsystem app
/// makes Windows allocate a console for it, which flashes visibly on screen
/// for the instant the child runs even though nothing ever needed to show
/// in it. Every `Command` that shells out to a console program from this
/// app should carry this.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
fn silent_command(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut command = std::process::Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(not(windows))]
fn silent_command(program: &str) -> std::process::Command {
    std::process::Command::new(program)
}

fn ensure_firewall_rule() {
    let Ok(exe_path) = std::env::current_exe() else {
        tracing::warn!("could not resolve this exe's own path, skipping firewall rule registration");
        return;
    };
    let exe_path = exe_path.display().to_string();
    let rule_name = "Warlay LAN Launcher";

    let _ = silent_command("netsh")
        .args([
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            &format!("name={rule_name}"),
        ])
        .output();

    let add_result = silent_command("netsh")
        .args([
            "advfirewall",
            "firewall",
            "add",
            "rule",
            &format!("name={rule_name}"),
            "dir=in",
            "action=allow",
            &format!("program={exe_path}"),
            "enable=yes",
            "profile=any",
        ])
        .output();

    match add_result {
        Ok(output) if output.status.success() => {
            tracing::info!("registered a Windows Firewall inbound-allow rule for this exe on all network profiles");
        }
        Ok(output) => {
            tracing::warn!(
                stderr = %String::from_utf8_lossy(&output.stderr),
                "netsh reported an error registering the firewall rule (not running as Administrator?) — joiners \
                 on a Public-profile network may not be able to receive the game connection"
            );
        }
        Err(error) => {
            tracing::warn!(%error, "failed to run netsh to register the firewall rule");
        }
    }
}

/// Focuses (and un-minimizes, and un-hides) the main window — what both
/// "Mostrar Warlay" and a left-click on the tray icon do. The window is
/// never destroyed while the app is running (see `run()`'s
/// `WindowEvent::CloseRequested` handler), so this always finds it.
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// The daemon is a separate process that runs elevated (see
/// `commands::daemon::spawn_daemon_elevated`), so this app -- which is not --
/// can neither kill it nor rely on it noticing we left: it would keep every
/// hub connection, the room's mesh, the game tunnel and the LAN capture
/// alive, and other players would keep seeing this user as present. "Salir"
/// is meant to be a real quit, so the daemon is asked to shut itself down
/// (it releases all of that, answers, and exits its process), and we wait
/// -- bounded, so quitting can never hang -- until it is really gone before
/// exiting ourselves.
fn quit_app(app: &AppHandle) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let client = app_handle.state::<AppState>().daemon_client.lock().unwrap().clone();
        if let Some(client) = client {
            shutdown_daemon(&client).await;
        }
        app_handle.exit(0);
    });
}

/// How long to wait for the daemon to accept the shutdown, and then to be
/// gone.
const DAEMON_ANSWER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
const DAEMON_EXIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

async fn shutdown_daemon(client: &daemon_ipc::DaemonClient) {
    match tokio::time::timeout(DAEMON_ANSWER_TIMEOUT, client.call(DaemonRequest::Shutdown)).await {
        Ok(Ok(warlay::protocol::ipc::DaemonResponse::Success { .. })) => {
            if !commands::daemon::wait_until_daemon_gone(DAEMON_EXIT_TIMEOUT).await {
                tracing::warn!("el agente aceptó apagarse pero sigue activo tras {DAEMON_EXIT_TIMEOUT:?}");
            }
        }
        // An older daemon that doesn't know `Shutdown` (it answers with an
        // error), or one that didn't answer in time: at least let go of
        // everything it holds, as this quit always did.
        other => {
            tracing::warn!(
                ?other,
                "el agente no aceptó apagarse, se le pide solo soltar las conexiones"
            );
            let _ = tokio::time::timeout(DAEMON_ANSWER_TIMEOUT, client.call(DaemonRequest::DisconnectAll)).await;
        }
    }
}

/// Builds the system tray icon so the app can keep running in the
/// background after the window's own close button is clicked (see `run()`'s
/// `WindowEvent::CloseRequested` handler, which hides rather than destroys
/// the window) — the tray icon, "Mostrar Warlay", or double-clicking it in
/// the taskbar-hidden state all bring the window back; "Salir" is the only
/// path that actually terminates the process, via `quit_app`
/// (bypasses the close-hides-instead-of-quits behavior entirely, since
/// that hook only intercepts a *window* close, not an app-level exit call).
fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Mostrar Warlay", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .expect("the app icon should already be embedded from tauri.conf.json's bundle config");

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Warlay LAN Launcher")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => quit_app(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// This is a windowed app launched by double-click with `panic = "abort"`
/// in release — with no hook, a panic anywhere before the tracing
/// subscriber exists (or on a build without a console at all) is completely
/// silent: the process just vanishes, with nothing anywhere to explain why.
/// A real report hit exactly this (a missing directory made the very first
/// line of `run()` panic) and needed remote step-by-step debugging to even
/// locate. This turns any future one into an immediate, readable message
/// instead.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let message = info.to_string();
        tracing::error!(%message, "panic, about to abort");
        unsafe {
            use windows::core::HSTRING;
            use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
            let _ = MessageBoxW(
                None,
                &HSTRING::from(format!(
                    "Warlay encontró un error inesperado y debe cerrarse:\n\n{message}"
                )),
                &HSTRING::from("Warlay"),
                MB_OK | MB_ICONERROR,
            );
        }
    }));
}

pub fn run() {
    // WebView2's Chromium compositor can hang indefinitely trying to set up
    // hardware-accelerated rendering on machines/VMs without a reliable
    // GPU (confirmed on a real report: the process ran fine -- database,
    // logging, tray icon, everything -- and even got as far as spawning
    // WebView2's own subprocesses, but the window itself never appeared,
    // forever, with 0% CPU; forcing software rendering made it show
    // immediately and reliably). Warlay's UI is a simple lobby/chat, not
    // anything GPU-bound, so trading a little rendering headroom for
    // working at all on VMs/RDP/older hardware is an easy call. Must be
    // set before Tauri creates the webview (i.e. before `.run()` below);
    // an env var already set by the user (e.g. for their own testing)
    // wins over this default.

    install_panic_hook();

    // Opened here, before the subscriber, rather than in `.setup()` (which
    // is where it lived before log levels became configurable) — the
    // stored console/file log levels need to seed the subscriber's initial
    // filters, and `.setup()`'s `AppHandle` doesn't exist yet at this
    // point. Moved into `AppState` further down once Tauri's own state
    // management is available.
    let store = ClientStore::open(&client_db_path()).expect("open the local database");
    let settings = store.get_settings().unwrap_or_default();

    // RUST_LOG, set, overrides both targets identically (matches this app's
    // previous single-shared-filter behavior); otherwise each target starts
    // at whatever was last saved via Ajustes → Registro (both "debug" on a
    // fresh install — see `AppSettingsRow::default`). Either way, the level
    // can be changed live afterwards without a restart — see `log_control`.
    let env_override = std::env::var("RUST_LOG").ok();
    let initial_console = env_override
        .clone()
        .unwrap_or_else(|| settings.console_log_level.clone());
    let initial_file = env_override.unwrap_or_else(|| settings.file_log_level.clone());

    let log_dir = log_cleanup::log_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join(log_file_name());
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("open the log file");
    let (file_writer, log_guard) = tracing_appender::non_blocking(SyncOnFlush(log_file));
    // Leaked deliberately: `run()`'s `.run()` call below blocks for the
    // whole process lifetime, so this would live that long anyway — leaking
    // just avoids threading a guard through Tauri's builder chain for it.
    Box::leak(Box::new(log_guard));

    // The frontend's live log panel ("mini terminal") drains this — see
    // log_bridge's docs. Registered as a channel *before* the subscriber is
    // built so no early log line (including "logging initialized" itself)
    // is lost, even though nothing reads from it until `.setup()` below
    // spawns the forwarding task.
    let mut log_rx = log_bridge::init_channel();

    // Two independent reload handles rather than one shared filter: the
    // "console" target (stdout + the in-app console drawer) and the log
    // file can be set to different levels and changed live, hence one
    // `reload::Layer` per `fmt::layer()` instead of one filter shared by
    // all three. Stdout and the channel writer share the same *level* but
    // stay separate handles/layers since they need different ANSI settings.
    let (console_stdout_filter, console_stdout_handle) =
        tracing_subscriber::reload::Layer::new(EnvFilter::new(&initial_console));
    let (console_channel_filter, console_channel_handle) =
        tracing_subscriber::reload::Layer::new(EnvFilter::new(&initial_console));
    let (file_filter, file_handle) = tracing_subscriber::reload::Layer::new(EnvFilter::new(&initial_file));

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(std::io::stdout)
                .with_filter(console_stdout_filter),
        )
        .with(
            fmt::layer()
                .with_writer(file_writer)
                .with_ansi(false)
                .with_filter(file_filter),
        )
        .with(
            fmt::layer()
                .with_writer(|| log_bridge::ChannelWriter)
                .with_ansi(false)
                .with_filter(console_channel_filter),
        )
        .init();
    log_control::init(
        console_stdout_handle.into(),
        console_channel_handle.into(),
        file_handle.into(),
    );
    tracing::info!(log_file = %log_path.display(), "logging initialized");

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    use tauri_plugin_global_shortcut::ShortcutState;
                    let state = app.state::<AppState>();
                    let is_mic_toggle = state
                        .mic_toggle_shortcut
                        .lock()
                        .expect("mic toggle shortcut lock poisoned")
                        .as_ref()
                        == Some(shortcut);
                    // Toggle on press only -- this shortcut is a switch, not
                    // a hold, so the release event (which still fires) is
                    // ignored to avoid toggling twice per key press.
                    if is_mic_toggle && event.state() == ShortcutState::Pressed {
                        let current = state.voice_manager.is_self_mic_muted();
                        let new_state = !current;
                        state.voice_manager.set_self_mic_muted(new_state);
                        let _ = app.emit("voice://self-mute-changed", new_state);
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let mic_toggle_key = settings.mic_toggle_key.clone();
            app.manage(AppState::new(store));
            let state = app.state::<AppState>();
            *state
                .mic_toggle_shortcut
                .lock()
                .expect("mic toggle shortcut lock poisoned") =
                voice::register_shortcut(app.handle(), None, &mic_toggle_key);
            // Non-fatal on purpose: the overlay is a cosmetic add-on, and
            // the rest of the app (matchmaking, LAN, chat) has no reason to
            // fail to start just because this window couldn't be created.
            if let Err(error) = overlay::setup_overlay(app.handle()) {
                tracing::warn!(%error, "failed to create overlay window, continuing without it");
            }

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(line) = log_rx.recv().await {
                    let _ = app_handle.emit("log://line", line);
                }
            });

            // Backstop for whatever the app's own launch/exit tracking
            // doesn't see: the configured game already running when this
            // app starts, or started/closed by the user outside our own
            // "Iniciar Frozen Throne" button. `launch_game`'s own optimistic
            // emits (see commands/launcher.rs) give instant feedback for the
            // common case; this just needs to eventually agree with reality,
            // hence the modest 2s interval rather than anything tighter.
            //
            // Compares against `AppState.game_running` (not a private local
            // variable) deliberately: `launch_game`'s optimistic emit and its
            // exit-watcher both write that same flag, so this poller's
            // change-detection agrees with theirs even for a transition it
            // never itself observed directly (e.g. a process launched and
            // killed again between two of this loop's ticks) -- a purely
            // local "last known" here would otherwise go stale forever in
            // that case, since nothing else would ever tell *it* the state
            // had changed.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
                loop {
                    interval.tick().await;
                    let state = app_handle.state::<AppState>();
                    let path = state
                        .store
                        .get_settings()
                        .map(|s| s.game_executable_path)
                        .unwrap_or_default();
                    // A job means this launch went through `launch_game`
                    // itself -- prefer it over name-matching since it
                    // tracks the launch's real process tree regardless of
                    // what any launcher-stub hand-off along the way is
                    // named (see `launcher::create_kill_on_close_job`).
                    // Falls back to the older name-matching check only for
                    // a game running outside Warlay's own launch button,
                    // where there's no job to check.
                    #[cfg(windows)]
                    let job = *state.game_job.lock().expect("game job lock poisoned");
                    #[cfg(not(windows))]
                    let job: Option<launcher::GameJob> = None;
                    let running = if let Some(job) = job {
                        !launcher::job_process_ids(job).is_empty()
                    } else if path.trim().is_empty() {
                        false
                    } else {
                        launcher::is_process_running_at(std::path::Path::new(&path))
                    };
                    let was_running = state.game_running.swap(running, std::sync::atomic::Ordering::SeqCst);
                    if running != was_running {
                        tracing::info!(running, "estado de Warcraft III cambió");
                        let _ = app_handle.emit("game://running-state-changed", running);
                        overlay::set_overlay_visible(&app_handle, running);
                    } else if running {
                        overlay::update_overlay_position(&app_handle, std::path::Path::new(&path));
                    }
                }
            });

            // A second, much faster loop dedicated purely to holding the
            // overlay at the top of the z-order. The poller above only
            // needs to recompute the overlay's position every couple of
            // seconds, but winning a z-order fight against the game
            // reclaiming topmost for itself on every focus change needs a
            // noticeably tighter cadence than that -- kept as its own loop
            // (instead of just tightening the interval above) so this can
            // run often without repeating the heavier process/window
            // enumeration work that poller also does. Reads the
            // already-computed `game_running` flag rather than checking
            // itself, so it stays cheap.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(400));
                loop {
                    interval.tick().await;
                    let state = app_handle.state::<AppState>();
                    if state.game_running.load(std::sync::atomic::Ordering::SeqCst) {
                        overlay::reassert_overlay_topmost(&app_handle);
                    }
                }
            });

            // At launch, then hourly: the app can sit in the tray for days.
            log_cleanup::spawn_periodic(app.handle().clone());

            setup_tray(app.handle())?;

            // Off the main/setup thread deliberately: this shells out to
            // `netsh advfirewall` twice and waits for it, which has no
            // guaranteed upper bound (e.g. the Windows Firewall service
            // being disabled/unresponsive, seen in some VM images, can hang
            // it indefinitely). It used to run synchronously before the
            // window was even created, so a hang here meant the whole app
            // looked "running but no window ever appears, forever" -- this
            // is best-effort ("failing here is non-fatal", per its own
            // docs), so it should never be able to block the one thing
            // (showing the window) that actually matters.
            std::thread::spawn(ensure_firewall_rule);

            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window (its own X button) hides it instead of
            // destroying it, so the process — and its live WS connections,
            // tunnels, and LAN capture tasks, none of which are tied to
            // window visibility — keeps running in the background. The
            // tray icon's "Mostrar Warlay" (or a left-click on it) is what
            // brings the window back; "Salir" from the tray menu is the
            // only way to actually quit.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::daemon::check_daemon_ready,
            commands::daemon::ensure_daemon_running,
            commands::devtools::open_devtools,
            commands::e2e::e2e_identity,
            commands::e2e::e2e_encrypt,
            commands::e2e::e2e_decrypt,
            commands::e2e::e2e_safety_number,
            commands::discovery::list_local_subnets,
            commands::discovery::discover_relays,
            commands::discovery::list_games,
            commands::relay::start_local_relay,
            commands::relay::stop_local_relay,
            commands::relay::watch_games,
            commands::relay::stop_watch_games,
            commands::launcher::launch_game,
            commands::launcher::path_exists,
            commands::launcher::is_game_running,
            commands::matchmaking::connect_to_server,
            commands::matchmaking::disconnect_from_server,
            commands::matchmaking::create_room,
            commands::matchmaking::join_room,
            commands::matchmaking::leave_room,
            commands::matchmaking::delete_room,
            commands::matchmaking::set_room_in_progress,
            commands::matchmaking::set_room_available,
            commands::matchmaking::send_chat_message,
            commands::matchmaking::send_private_message,
            commands::matchmaking::start_hosting_tunnel,
            commands::matchmaking::start_joining_tunnel,
            commands::matchmaking::stop_hub_tunnel,
            commands::matchmaking::get_room_members,
            commands::servers::list_servers,
            commands::servers::add_server,
            commands::servers::remove_server,
            commands::servers::get_client_id,
            commands::settings::load_settings,
            commands::settings::save_settings,
            commands::settings::set_log_level,
            commands::settings::set_log_retention_days,
            commands::settings::get_log_usage,
            commands::settings::clean_logs_now,
            commands::system_info::get_system_info,
            commands::overlay::set_overlay_interactive,
            commands::voice::push_voice_frame,
            commands::voice::start_voice_chat,
            commands::voice::stop_voice_chat,
            commands::voice::toggle_mute_peer,
            commands::voice::get_muted_peers,
            commands::voice::toggle_self_mic_mute,
            commands::voice::is_self_mic_muted,
            commands::agent::get_agent_settings,
            commands::agent::save_agent_settings,
            commands::agent::get_agent_status,
            commands::agent::retry_peer_mesh,
            commands::session::start_room_session,
            commands::session::stop_room_session,
            commands::session::start_game_tunnel,
            commands::session::set_overlay_roster,
            commands::session::update_room_helper,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Warlay LAN Launcher application");
}
