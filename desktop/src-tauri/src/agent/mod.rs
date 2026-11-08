//! The Warlay network agent -- everything the old standalone agent did,
//! as a library.
//!
//! The agent is the half of Warlay that needs elevated privileges or
//! long-lived sockets: hub connections, LAN capture/replay, the P2P game
//! tunnel, the voice relay. It speaks the pipe protocol defined in
//! [`warlay::protocol::ipc`] and does its networking through
//! [`warlay::engine`]; nothing in this crate re-implements either.
//!
//! The agent runs *inside* the desktop client process now
//! (`desktop/src-tauri`). There is no separate agent sidecar and no named
//! pipe: the desktop client's commands call the agent's handlers directly.
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
pub mod status;
pub mod storage;
pub mod voice;

/// Exit code used when the agent refuses to run without elevation.
pub const EXIT_NOT_ELEVATED: i32 = 2;

/// The prefix used for the agent's log files, so they can be told apart
/// from the desktop client's own `warlay-*.log` files in the same directory.
pub const LOG_FILE_PREFIX: &str = "warlaid";
