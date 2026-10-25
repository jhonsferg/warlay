#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // One binary, one role: the desktop client runs the Warlay network agent
    // inside its own process (see `src/agent/`). No `--daemon` flag, no
    // second process, no named pipe.
    desktop_lib::run();
}
