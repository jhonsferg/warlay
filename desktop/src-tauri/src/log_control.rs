//! Lets the "console" log level (stdout + the in-app console drawer — both
//! surfaces someone watching the app live can see) and the log-file level
//! be changed while the app is running, with no restart. Mirrors
//! `log_bridge`'s `static ... OnceLock` pattern: the reload handles are
//! created in `lib.rs::run()` before Tauri's own managed state exists, so a
//! plain `OnceLock` is the simplest way for a later Tauri command
//! (`commands::settings::set_log_level`) to reach them.
use std::sync::OnceLock;
use tracing_subscriber::{reload, EnvFilter};

/// Type-erases a `reload::Handle<EnvFilter, S>` — `S` is whatever the
/// subscriber stack looks like *up to this layer's position* at the point
/// `reload::Layer::new` was called (a different, uglier `Layered<...>` type
/// for each of the three layers in `lib.rs`, since each is nested one level
/// deeper than the last), which is impossible to name as a single struct
/// field type. Boxing behind this closure is what lets `LogHandles` hold
/// three otherwise-incompatible handle types uniformly.
pub struct ReloadHandle(Box<dyn Fn(EnvFilter) -> Result<(), String> + Send + Sync>);

impl<S: 'static> From<reload::Handle<EnvFilter, S>> for ReloadHandle {
    fn from(handle: reload::Handle<EnvFilter, S>) -> Self {
        ReloadHandle(Box::new(move |filter| {
            handle.reload(filter).map_err(|error| error.to_string())
        }))
    }
}

impl ReloadHandle {
    fn apply(&self, filter: EnvFilter) -> Result<(), String> {
        (self.0)(filter)
    }
}

struct LogHandles {
    /// Two handles rather than one shared filter object for the "console"
    /// target, since stdout and the in-app console drawer stay separate
    /// `fmt::layer()`s (they need different ANSI settings — a terminal
    /// wants color, the UI panel doesn't) but should always move together
    /// as one user-facing level.
    console_stdout: ReloadHandle,
    console_channel: ReloadHandle,
    file: ReloadHandle,
}

static HANDLES: OnceLock<LogHandles> = OnceLock::new();

/// Call exactly once, right after the subscriber that owns these handles is
/// installed.
pub fn init(console_stdout: ReloadHandle, console_channel: ReloadHandle, file: ReloadHandle) {
    HANDLES
        .set(LogHandles {
            console_stdout,
            console_channel,
            file,
        })
        .ok()
        .expect("log_control::init called twice");
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogTarget {
    Console,
    File,
}

/// Applies a new `tracing_subscriber::EnvFilter` directive (e.g. `"info"`,
/// `"warn"`, or a scoped directive like `"lan_launcher_client=trace"`) to
/// the given target immediately — every log line from this point on is
/// affected, no restart needed. Rejects an unparseable directive instead of
/// silently ignoring it, so a typo in the settings UI surfaces as an error
/// rather than a level change that quietly didn't happen.
pub fn set_level(target: LogTarget, level: &str) -> Result<(), String> {
    let handles = HANDLES.get().expect("log_control::init not called");
    match target {
        LogTarget::Console => {
            let filter = EnvFilter::try_new(level).map_err(|error| error.to_string())?;
            handles.console_stdout.apply(filter)?;
            let filter = EnvFilter::try_new(level).map_err(|error| error.to_string())?;
            handles.console_channel.apply(filter)
        }
        LogTarget::File => {
            let filter = EnvFilter::try_new(level).map_err(|error| error.to_string())?;
            handles.file.apply(filter)
        }
    }
}
