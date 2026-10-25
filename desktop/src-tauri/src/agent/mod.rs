//! The Warlay network agent -- everything `wnetd.exe` does, as a library.
//!
//! The agent is the half of Warlay that needs elevated privileges or
//! long-lived sockets: hub connections, LAN capture/replay, the P2P game
//! tunnel, the voice relay. It speaks the pipe protocol defined in
//! [`warlay::protocol::ipc`] and does its networking through
//! [`warlay::engine`]; nothing in this crate re-implements either.
//!
//! The agent runs *inside* the desktop client process now
//! (`desktop/src-tauri`). There is no separate `wnetd.exe` sidecar and no
//! named pipe: the desktop client's commands call the agent's handlers
//! directly.
//!
//! # Process-wide globals (read before touching this from a third host)
//!
//! [`logging::init_logging`] installs the global tracing subscriber and panic
//! hook, [`log_control::init`] and [`shutdown::request`] each own a
//! `OnceLock`. That is fatal for two callers in one process: a second
//! `Invocation::run()` in a process that already logs through `tracing`
//! would panic. Call it at most once per process.

#[cfg(windows)]
pub mod autostart;
#[cfg(windows)]
pub mod elevation;
pub mod events;
pub mod handlers;
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

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{error, info};
use warlay::protocol::ipc::AgentSettings;

use self::state::DaemonState;
use self::storage::AgentStore;

/// Exit code used when the agent refuses to run without elevation. Kept for
// compatibility with the old standalone `wnetd.exe` entry point.
pub const EXIT_NOT_ELEVATED: i32 = 2;

/// What a process was asked to do with the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invocation {
    /// Serve the agent over the IPC pipe until asked to stop.
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
    pub fn run(self) -> i32 {
        match self {
            // The agent now lives inside the desktop client process; there is
            // no separate daemon process to start from here.
            Invocation::Daemon => 0,
            Invocation::Install => install(),
            Invocation::Uninstall => uninstall(),
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

#[cfg(test)]
mod tests {
    use super::{parse_invocation, Invocation};

    /// Builds an argv: `first` is `argv[0]`, the rest are the flags.
    fn argv(first: &str, rest: &[&str]) -> Vec<String> {
        std::iter::once(first.to_owned())
            .chain(rest.iter().map(|flag| (*flag).to_owned()))
            .collect()
    }

    #[test]
    fn no_flag_is_the_desktop_client() {
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
        assert_eq!(parse_invocation(&argv("warlay.exe", &["something", "--daemon"])), None);
    }
}
