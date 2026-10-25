#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // This binary has two roles: the desktop client, and the Warlay network
    // agent it used to ship as a separate wnetd.exe. Which one it is comes
    // down to the first argument, and the branch has to happen HERE, before
    // `run()` installs anything process-wide: the agent brings its own
    // tracing subscriber, panic hook, log control and shutdown signal, and a
    // second subscriber in the same process panics outright.
    //
    // In agent mode nothing Tauri-related is touched at all - no window, no
    // tray, no webview - and the release build's `windows` subsystem (above)
    // keeps it from flashing a console either. See `crates/agent`.
    let args: Vec<String> = std::env::args().collect();
    if let Some(invocation) = warlay_agent::parse_invocation(&args) {
        std::process::exit(invocation.run());
    }

    desktop_lib::run();
}
