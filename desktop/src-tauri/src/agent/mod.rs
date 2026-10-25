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
pub mod elevation;
pub mod events;
pub mod handlers;
pub mod lan;
pub mod log_control;
pub mod logging;
pub mod paths;
pub mod session;
pub mod shutdown;
pub mod state;
pub mod storage;

use tracing::{error, info};
use warlay::protocol::ipc::AgentSettings;

use self::state::DaemonState;
use self::storage::AgentStore;

/// Exit code used when the agent refuses to run without elevation. Kept for
/// compatibility with the old standalone `wnetd.exe` entry point.
pub const EXIT_NOT_ELEVATED: i32 = 2;

/// What a process was asked to do with the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invocation {
    /// Run the agent inside the current process (desktop client mode).
    Daemon,
}

/// Reads an agent request out of `args`, or reports `None` when these
/// arguments belong to something else -- notably the desktop client's normal
/// double-click start, which must carry on launching the GUI.
///
/// Only `argv[1]` is inspected.
pub fn parse_invocation(args: &[String]) -> Option<Invocation> {
    match args.get(1).map(String::as_str) {
        Some("--daemon") => Some(Invocation::Daemon),
        _ => None,
    }
}

impl Invocation {
    /// Runs the request to completion and returns the process exit code.
    pub fn run(self) -> i32 {
        // The agent now lives inside the desktop client process; there is
        // no separate daemon process to start from here.
        let _ = self;
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
    fn daemon_flag_is_recognised() {
        assert_eq!(
            parse_invocation(&argv("warlay.exe", &["--daemon"])),
            Some(Invocation::Daemon)
        );
        assert_eq!(parse_invocation(&argv("warlay.exe", &[])), None);
    }

    #[test]
    fn only_the_first_flag_is_read() {
        assert_eq!(parse_invocation(&argv("warlay.exe", &["something", "--daemon"])), None);
    }
}
