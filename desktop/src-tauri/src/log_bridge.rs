//! Feeds every log line this app produces into a channel the frontend can
//! drain and show live — the "mini terminal" in the UI. Reuses
//! `tracing_subscriber::fmt`'s own formatter (via a custom writer target)
//! rather than hand-rolling one, so the frontend sees exactly the same text
//! as the log file and the console, just delivered differently.
use std::sync::OnceLock;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

static SENDER: OnceLock<UnboundedSender<String>> = OnceLock::new();

/// Call once, before `tracing_subscriber::registry().init()` — returns the
/// receiving end the setup code should drain and forward to the frontend.
pub fn init_channel() -> UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded_channel();
    SENDER.set(tx).ok().expect("log_bridge::init_channel called twice");
    rx
}

/// A `tracing_subscriber::fmt::MakeWriter` target: every formatted line
/// `fmt::layer()` would otherwise print gets sent down the channel instead
/// (in addition to — this is layered alongside, not instead of — stdout and
/// the log file).
#[derive(Clone, Copy, Default)]
pub struct ChannelWriter;

impl std::io::Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Some(sender) = SENDER.get() {
            let _ = sender.send(String::from_utf8_lossy(buf).into_owned());
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
