//! Shutdown signal for every server mode.
//!
//! `docker stop` (and a container replacement) sends SIGTERM. The servers run
//! as PID 1 in their container, where a signal nobody handles is ignored, so
//! with only Ctrl+C handled they kept running until Docker gave up after ten
//! seconds and killed them. Listening for SIGTERM too makes a replacement
//! prompt: the old process stops at once and the new one takes over.

/// Resolves when the process is asked to stop (SIGINT or SIGTERM on Unix,
/// Ctrl+C elsewhere).
pub async fn signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("install the SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
