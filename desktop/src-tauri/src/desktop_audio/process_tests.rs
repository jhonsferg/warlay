use super::*;
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// The tests look for `ping` among THIS process's children, so they must not
/// run side by side (each would find the other's child).
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}

fn spawn_ping() -> std::process::Child {
    Command::new("ping")
        .args(["-n", "30", "127.0.0.1"])
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn ping")
}

#[test]
fn finds_a_direct_child_by_name_ignoring_case() {
    let _guard = serial();
    let mut child = spawn_ping();
    let me = std::process::id();
    assert_eq!(child_process_named(me, "PING.EXE"), Some(child.id()));
    assert_eq!(child_process_named(me, "ping.exe"), Some(child.id()));
    let _ = child.kill();
}

#[test]
fn does_not_report_other_names_or_other_parents() {
    let _guard = serial();
    let mut child = spawn_ping();
    let me = std::process::id();
    assert_eq!(child_process_named(me, "msedgewebview2.exe"), None);
    // `ping` is a child of this test, not of that (arbitrary) other process.
    assert_eq!(child_process_named(me.wrapping_add(7), "ping.exe"), None);
    assert_eq!(child_process_named(0xFFFF_FFF0, "ping.exe"), None);
    let _ = child.kill();
}

#[test]
fn a_finished_child_is_no_longer_found() {
    let _guard = serial();
    let mut child = spawn_ping();
    let me = std::process::id();
    let _ = child.kill();
    let _ = child.wait();
    // The process object lingers (and stays listed) while we hold its handle.
    drop(child);
    let gone = (0..20).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(50));
        child_process_named(me, "ping.exe").is_none()
    });
    assert!(gone, "a finished child is still reported");
}

#[test]
fn without_a_webview_child_the_whole_app_is_excluded() {
    // The test process has no msedgewebview2 child: the safe fallback applies.
    assert_eq!(exclusion_pid(), std::process::id());
}
