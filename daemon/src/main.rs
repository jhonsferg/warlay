// A "console" subsystem binary (the default) gets Windows to attach/allocate
// a visible console window on every launch, whether double-clicked, started
// by the logon Scheduled Task, or spawned by desktop.exe -- exactly the
// opposite of "proceso discreto." Release builds link against the "windows"
// subsystem instead (same mechanism Tauri's own desktop.exe already uses),
// so the daemon never shows a window regardless of how it's started; debug
// builds keep the console since that's the only visibility into `tracing`'s
// stdout output while developing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod handlers;
mod ipc_server;
mod lan;
mod log_control;
mod logging;
mod security;
mod state;
mod storage;

use std::collections::HashMap;
use std::io::Error;
use std::sync::Arc;

use tokio::net::windows::named_pipe::ServerOptions;
use tokio::sync::Mutex;
use tracing::{info, warn};

use state::DaemonState;
use storage::AgentStore;

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Opened before the subscriber -- its `log_level` seeds the initial
    // filters (see `logging::init_logging`'s doc comment), same ordering
    // constraint desktop's own `ClientStore` has relative to its subscriber.
    let agent_store = AgentStore::open();
    let agent_settings = agent_store.get_settings();
    logging::init_logging(&agent_settings.log_level);

    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--install") => return autostart::install_autostart(),
        Some("--uninstall") => return autostart::uninstall_autostart(),
        _ => {}
    }

    info!("Warlay daemon started");

    let pipe_name = r"\\.\pipe\warlay-daemon";
    let pipe_sa_addr = security::build_pipe_security_attributes();

    let mut server = unsafe {
        ServerOptions::new()
            .first_pipe_instance(true)
            .create_with_security_attributes_raw(pipe_name, pipe_sa_addr as *mut std::ffi::c_void)
    }?;

    let daemon_state = Arc::new(Mutex::new(DaemonState {
        connections: HashMap::new(),
        game_tunnel: None,
        peer_mesh: None,
        voice_poll_task: None,
        lan_capture: None,
        lan_rebroadcast: None,
        lan_relay_tx: None,
        lan_loop_guard: Arc::new(warlay::engine::LoopGuard::new(std::time::Duration::from_secs(2))),
        active_room_server: None,
        active_room_id: None,
        current_tx: None,
        game_started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        lan_discovery_enabled: Arc::new(std::sync::atomic::AtomicBool::new(agent_settings.lan_discovery_enabled)),
        agent_store,
        started_at: std::time::Instant::now(),
    }));

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
                // installation's warlay.exe.
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
        }
    }

    info!("Warlay daemon stopped");
    Ok(())
}
