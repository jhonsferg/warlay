use super::*;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

/// The tests spawn `ping` children of THIS process and look them up by name.
/// A `ping` killed a moment ago can still be listed on a slow machine, so every
/// check is about OUR child's pid being (or not being) among the matches --
/// never about which `ping` happens to come first -- and the tests run one at a
/// time.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}

fn spawn_ping() -> Child {
    Command::new("ping")
        .args(["-n", "30", "127.0.0.1"])
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn ping")
}

/// Kills the child and waits until it is really gone (and its handle closed).
fn reap(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Retries for up to a second: the process list is a snapshot and can lag.
fn eventually(mut condition: impl FnMut() -> bool) -> bool {
    (0..20).any(|_| {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
        false
    })
}

#[test]
fn finds_a_direct_child_by_name_ignoring_case() {
    let _guard = serial();
    let child = spawn_ping();
    let (me, pid) = (std::process::id(), child.id());
    assert!(eventually(|| children_named(me, "PING.EXE").contains(&pid)));
    assert!(eventually(|| children_named(me, "ping.exe").contains(&pid)));
    assert!(child_process_named(me, "ping.exe").is_some());
    reap(child);
}

#[test]
fn does_not_report_other_names_or_other_parents() {
    let _guard = serial();
    let child = spawn_ping();
    let (me, pid) = (std::process::id(), child.id());
    assert!(eventually(|| children_named(me, "ping.exe").contains(&pid)));
    assert!(children_named(me, "msedgewebview2.exe").is_empty());
    assert_eq!(child_process_named(me, "msedgewebview2.exe"), None);
    // `ping` is a child of this test, not of those (arbitrary) other processes.
    assert!(!children_named(me.wrapping_add(7), "ping.exe").contains(&pid));
    assert!(children_named(0xFFFF_FFF0, "ping.exe").is_empty());
    reap(child);
}

#[test]
fn a_finished_child_is_no_longer_found() {
    let _guard = serial();
    let child = spawn_ping();
    let (me, pid) = (std::process::id(), child.id());
    assert!(eventually(|| children_named(me, "ping.exe").contains(&pid)));
    // The process object lingers (and stays listed) while a handle is open.
    reap(child);
    assert!(
        eventually(|| !children_named(me, "ping.exe").contains(&pid)),
        "a finished child is still reported"
    );
}

#[test]
fn without_a_webview_child_the_whole_app_is_excluded() {
    // The test process has no msedgewebview2 child: the safe fallback applies.
    assert_eq!(exclusion_pid(), std::process::id());
}
