// A "console" subsystem binary (the default) gets Windows to attach/allocate
// a visible console window on every launch, whether double-clicked, started
// by the logon Scheduled Task, or spawned by desktop.exe -- exactly the
// opposite of "proceso discreto." Release builds link against the "windows"
// subsystem instead (same mechanism Tauri's own desktop.exe already uses),
// so the daemon never shows a window regardless of how it's started; debug
// builds keep the console since that's the only visibility into `tracing`'s
// stdout output while developing. (Ignored on other platforms.)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Standalone `wnetd.exe`: the Warlay network agent as its own process.
//!
//! All of the behaviour lives in the `agent` crate -- this binary is only an
//! entry point around it, so that `warlay.exe --daemon` (which embeds the
//! very same crate) and `wnetd.exe` cannot drift apart. Build it with
//! `cargo build -p daemon`.

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // `wnetd` with no arguments *is* the agent, as it always has been;
    // `--install`/`--uninstall` do what they say, and `--daemon` (the flag
    // warlay.exe uses for itself) selects the same thing explicitly.
    let invocation = warlay_agent::parse_invocation(&args).unwrap_or(warlay_agent::Invocation::Daemon);

    std::process::exit(invocation.run());
}
