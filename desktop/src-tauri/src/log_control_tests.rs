use super::*;
use tracing_subscriber::prelude::*;

/// `HANDLES` is a process-wide `OnceLock` that may be set exactly once, so
/// everything about it lives in this single test.
#[test]
fn levels_apply_live_and_a_bad_directive_is_rejected() {
    let (console_a, handle_a) = reload::Layer::new(EnvFilter::new("info"));
    let (console_b, handle_b) = reload::Layer::new(EnvFilter::new("info"));
    let (file, handle_f) = reload::Layer::new(EnvFilter::new("debug"));
    let _subscriber = tracing_subscriber::registry().with(console_a).with(console_b).with(file);
    init(handle_a.clone().into(), handle_b.into(), handle_f.clone().into());

    set_level(LogTarget::Console, "warn").unwrap();
    set_level(LogTarget::File, "trace").unwrap();
    set_level(LogTarget::Console, "engine=trace,info").unwrap();

    let applied = handle_f.with_current(|f| f.to_string()).unwrap();
    assert_eq!(applied, "trace");
    let console = handle_a.with_current(|f| f.to_string()).unwrap();
    assert!(console.contains("engine=trace"), "{console}");

    let err = set_level(LogTarget::Console, "=== no ===[").unwrap_err();
    assert!(!err.is_empty());
    // A rejected directive leaves the previous level in place.
    assert!(handle_a.with_current(|f| f.to_string()).unwrap().contains("engine=trace"));
}
