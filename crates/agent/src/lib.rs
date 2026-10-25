//! The Warlay network agent -- everything `wnetd.exe` does, as a library.
//!
//! The agent is the half of Warlay that needs elevated privileges or
//! long-lived sockets: hub connections, LAN capture/replay, the P2P game
//! tunnel, the voice relay. It speaks the pipe protocol defined in
//! [`warlay::protocol::ipc`] and does its networking through
//! [`warlay::engine`]; nothing in this crate re-implements either.
//!
//! It has two hosts, and they run exactly the same code:
//!
//! * `daemon/src/main.rs` -- the standalone `wnetd.exe`, a thin wrapper kept
//!   for development, CI and manual runs (`cargo build -p daemon`).
//! * `desktop/src-tauri/src/main.rs` -- `warlay.exe` dispatches to
//!   [`Invocation::Daemon`] when started with `--daemon`, which is what lets
//!   a single distributed binary also be the daemon.
//!
//! Both are ordinary entry points into [`Invocation::run`]; there is no
//! "daemon mode" flag to keep in sync between them because the mode *is*
//! this crate.
//!
//! # Process-wide globals (read before touching this from a third host)
//!
//! [`logging::init_logging`] installs the global tracing subscriber and panic
//! hook, [`log_control::init`] and [`shutdown::request`] each own a
//! `OnceLock`, and the pipe is created with `first_pipe_instance(true)`.
//! That is fine for two *processes* sharing this crate -- each gets its own
//! copy of every global -- and fatal for two callers in one process: a second
//! `Invocation::run()` in a process that already logs through `tracing`
//! would panic. Call it at most once per process, before any other
//! subscriber is installed.

#[cfg(windows)]
pub mod autostart;
#[cfg(windows)]
pub mod elevation;
pub mod handlers;
pub mod ipc_server;
pub mod lan;
pub mod log_control;
pub mod logging;
pub mod paths;
#[cfg(windows)]
pub mod security;
pub mod session;
pub mod shutdown;
pub mod state;
pub mod storage;
#[cfg(unix)]
pub mod unix_socket;

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{error, info};
use warlay::protocol::ipc::AgentSettings;

use state::DaemonState;
use storage::AgentStore;

/// Exit code used when the agent refuses to run without elevation. The
/// desktop client doesn't inspect it (it only ever sees "the pipe never
/// appeared"), but a distinct code keeps the log and any wrapper script
/// honest about *why* it gave up.
pub const EXIT_NOT_ELEVATED: i32 = 2;

/// What a process was asked to do with the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invocation {
    /// Serves the agent over the IPC pipe until asked to stop.
    Daemon,
    /// Register the logon auto-start task (Windows).
    Install,
    /// Remove the logon auto-start task (Windows).
    Uninstall,
}

/// Reads an agent request out of `args`, or reports `None` when these
/// arguments belong to something else -- notably the desktop client's normal
/// double-click start, which must carry on launching the GUI.
///
/// Only `argv[1]` is inspected, matching how the daemon binary has always
/// parsed its flags: `wnetd --install`, `warlay --daemon`.
pub fn parse_invocation(args: &[String]) -> Option<Invocation> {
    match args.get(1).map(String::as_str) {
        Some("--daemon") => Some(Invocation::Daemon),
        Some("--install") => Some(Invocation::Install),
        Some("--uninstall") => Some(Invocation::Uninstall),
        _ => None,
    }
}

impl Invocation {
    /// Runs the request to completion and returns the process exit code.
    ///
    /// Ordering matters and mirrors the old `daemon/src/main.rs`: the agent
    /// store is opened *before* the logging subscriber because the stored
    /// log level seeds the subscriber's initial filter.
    pub fn run(self) -> i32 {
        let agent_store = AgentStore::open();
        let agent_settings = agent_store.get_settings();
        logging::init_logging(&agent_settings.log_level);

        match self {
            Invocation::Daemon => run_daemon(agent_store, agent_settings),
            Invocation::Install => install(),
            Invocation::Uninstall => uninstall(),
        }
    }
}

/// Runs the agent's accept loop on a dedicated Tokio runtime and returns
/// when the pipe server stops: `Ctrl+C`, a `Shutdown` request from the
/// desktop app, or an error (including a second agent trying to take a pipe
/// that is already served).
fn run_daemon(agent_store: AgentStore, agent_settings: AgentSettings) -> i32 {
    // Last line of defence behind the `requireAdministrator` manifest each
    // of our binaries embeds: Windows should refuse to start this image
    // unelevated in the first place. If something did start it anyway, bail
    // out *before* the pipe exists rather than serving an agent that only
    // reveals it can't open the raw capture socket on the first game.
    #[cfg(windows)]
    if !elevation::is_elevated() {
        error!(
            "el agente arrancó sin privilegios de administrador; \
             la captura de LAN (SIO_RCVALL) fallaría, saliendo sin abrir el pipe"
        );
        return EXIT_NOT_ELEVATED;
    }

    info!("Warlay daemon started");

    let daemon_state = Arc::new(Mutex::new(DaemonState {
        connections: HashMap::new(),
        game_tunnel: None,
        peer_mesh: None,
        room_session: None,
        voice_poll_task: None,
        lan_capture: None,
        lan_rebroadcast: None,
        lan_relay_tx: None,
        lan_loop_guard: Arc::new(warlay::engine::LoopGuard::new(std::time::Duration::from_secs(2))),
        lan_capture_ports: warlay::engine::CapturePorts::default(),
        own_game_hosting: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        game_gateway: None,
        active_room_server: None,
        active_room_id: None,
        current_tx: None,
        game_started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        lan_discovery_enabled: Arc::new(std::sync::atomic::AtomicBool::new(agent_settings.lan_discovery_enabled)),
        agent_store,
        started_at: std::time::Instant::now(),
    }));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("crear el runtime de tokio del agente");

    #[cfg(windows)]
    let result = runtime.block_on(serve_named_pipe(daemon_state));
    #[cfg(unix)]
    let result = runtime.block_on(unix_socket::serve(daemon_state));

    match result {
        Ok(()) => {
            info!("Warlay daemon stopped");
            0
        }
        Err(e) => {
            error!(error = %e, "el agente terminó con un error");
            1
        }
    }
}

fn install() -> i32 {
    #[cfg(windows)]
    match autostart::install_autostart() {
        Ok(()) => 0,
        Err(e) => {
            error!(error = %e, "no se pudo registrar el arranque automático");
            1
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("auto-start is managed by the service manager here (e.g. a systemd unit running `wnetd`)");
        0
    }
}

fn uninstall() -> i32 {
    #[cfg(windows)]
    match autostart::uninstall_autostart() {
        Ok(()) => 0,
        Err(e) => {
            error!(error = %e, "no se pudo quitar el arranque automático");
            1
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("auto-start is managed by the service manager here (e.g. a systemd unit running `wnetd`)");
        0
    }
}

/// Serves the IPC pipe until the process should exit.
///
/// The pipe's ACL only restricts by Windows *principal*, not by process, so
/// every connection is checked against this installation's own binary before
/// a single request is read from it (see `security`).
#[cfg(windows)]
async fn serve_named_pipe(daemon_state: Arc<Mutex<DaemonState>>) -> std::io::Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    use tracing::warn;

    let pipe_name = r"\\.\pipe\warlay-daemon";
    let pipe_sa_addr = security::build_pipe_security_attributes();

    let mut server = unsafe {
        ServerOptions::new()
            .first_pipe_instance(true)
            .create_with_security_attributes_raw(pipe_name, pipe_sa_addr as *mut std::ffi::c_void)
    }?;

    info!(%pipe_name, "listening on named pipe");

    loop {
        tokio::select! {
            accepted = server.connect() => {
                accepted?;

                let connected_client = server;
                // Prepare a new pipe for the NEXT client so we can accept more connections
                server = unsafe {
                    ServerOptions::new()
                        .create_with_security_attributes_raw(pipe_name, pipe_sa_addr as *mut std::ffi::c_void)
                }?;

                // The pipe ACL only restricts by Windows principal, not by
                // process -- see `security::verify_client_process`'s doc
                // comment. Reject and drop the connection outright before
                // ever reading a single request from it if it isn't our own
                // installation's client.
                if !security::verify_client_process(&connected_client) {
                    warn!("dropping pipe connection from unverified process");
                    continue;
                }

                info!("client connected to daemon");
                let state_clone = daemon_state.clone();
                tokio::spawn(ipc_server::handle_connection(connected_client, state_clone));
            }
            _ = tokio::signal::ctrl_c() => {
                info!("shutdown signal received, tearing down active session");
                let mut s = daemon_state.lock().await;
                state::teardown_tunnel_and_lan(&mut s);
                break;
            }
            // The desktop app asked us to exit (its tray "Salir"): the
            // handler already released everything, this just ends the loop.
            () = shutdown::requested() => {
                info!("shutdown requested by the desktop app, exiting");
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_invocation, Invocation};

    /// Builds an argv: `first` is `argv[0]`, the rest are the flags. The
    /// binary name is passed in because it differs between the two hosts
    /// (`warlay.exe` and `wnetd.exe`) and neither may be special-cased.
    fn argv(first: &str, rest: &[&str]) -> Vec<String> {
        std::iter::once(first.to_owned())
            .chain(rest.iter().map(|flag| (*flag).to_owned()))
            .collect()
    }

    #[test]
    fn no_flag_is_the_desktop_client() {
        // Double-clicking warlay.exe (or `cargo run -p desktop`) has to keep
        // reaching the GUI: an empty or unknown first argument is not an
        // agent request.
        assert_eq!(parse_invocation(&argv("warlay.exe", &[])), None);
        assert_eq!(parse_invocation(&argv("warlay.exe", &["--verbose"])), None);
        assert_eq!(parse_invocation(&[]), None);
    }

    #[test]
    fn agent_flags_are_recognised_in_both_hosts() {
        assert_eq!(
            parse_invocation(&argv("warlay.exe", &["--daemon"])),
            Some(Invocation::Daemon)
        );
        assert_eq!(
            parse_invocation(&argv("wnetd.exe", &["--daemon"])),
            Some(Invocation::Daemon)
        );
        assert_eq!(parse_invocation(&argv("wnetd.exe", &[])), None);
        assert_eq!(
            parse_invocation(&argv("warlay.exe", &["--install"])),
            Some(Invocation::Install)
        );
        assert_eq!(
            parse_invocation(&argv("warlay.exe", &["--uninstall"])),
            Some(Invocation::Uninstall)
        );
    }

    #[test]
    fn only_the_first_flag_is_read() {
        // Mirrors the flag parsing the daemon binary always had: anything
        // past argv[1] belongs to somebody else (a game started by the
        // client, for instance), never to us.
        assert_eq!(parse_invocation(&argv("warlay.exe", &["something", "--daemon"])), None);
    }
}
