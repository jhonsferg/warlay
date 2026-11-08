#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // One binary, one process: the UI and the network runtime (see
    // `src/runtime/`) both live in `warlay.exe`.
    desktop_lib::run();
}
