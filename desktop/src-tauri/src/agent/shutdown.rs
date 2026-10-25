//! A process-wide "please exit" signal. The daemon runs elevated, so the
//! desktop app (which is not) can't terminate it from outside; instead it
//! sends `DaemonRequest::Shutdown`, whose handler raises this signal and the
//! accept loop in `main.rs` / `unix_socket.rs` stops, which ends the process.

use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::Notify;

/// How long the reply to a `Shutdown` request gets to reach the client before
/// the process starts going away.
const REPLY_GRACE: Duration = Duration::from_millis(250);

fn signal() -> &'static Notify {
    static SIGNAL: OnceLock<Notify> = OnceLock::new();
    SIGNAL.get_or_init(Notify::new)
}

/// Resolves once a shutdown has been requested -- including one requested
/// before this was first awaited.
pub async fn requested() {
    signal().notified().await;
}

/// Asks the daemon to exit.
pub fn request() {
    signal().notify_one();
}

/// Like [`request`], after a moment: lets the handler that is answering the
/// request deliver its reply first, so the caller learns the daemon accepted
/// the shutdown rather than just seeing the connection vanish.
pub fn request_soon() {
    tokio::spawn(async {
        tokio::time::sleep(REPLY_GRACE).await;
        request();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_request_is_seen_whether_it_comes_before_or_after_waiting() {
        // The process-wide signal is shared, so both orders run in one test.
        request();
        tokio::time::timeout(Duration::from_secs(1), requested())
            .await
            .expect("a request made before waiting must not be lost");

        let waiter = tokio::spawn(requested());
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished(), "nothing was requested yet");
        request_soon();
        tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .expect("the delayed request must arrive")
            .unwrap();
    }
}
