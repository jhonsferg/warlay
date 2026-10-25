//! Lets the daemon's log level (both the stdout layer -- dev builds only,
//! see `main.rs`'s `windows_subsystem` doc comment -- and the file layer)
//! be changed while the process is running, with no restart. Mirrors
//! `desktop/src-tauri/src/log_control.rs`'s `ReloadHandle`/`OnceLock`
//! pattern, simplified to a single unified level: unlike the desktop
//! client, the daemon has no visible console for a "console vs file" split
//! to mean anything -- one level, applied to both handles together.
use std::sync::OnceLock;
use tracing_subscriber::{reload, EnvFilter};

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
    stdout: ReloadHandle,
    file: ReloadHandle,
}

static HANDLES: OnceLock<LogHandles> = OnceLock::new();

/// Call exactly once, right after the subscriber that owns these handles is
/// installed.
pub fn init(stdout: ReloadHandle, file: ReloadHandle) {
    HANDLES
        .set(LogHandles { stdout, file })
        .ok()
        .expect("log_control::init called twice");
}

/// Applies a new `tracing_subscriber::EnvFilter` directive (e.g. `"info"`,
/// `"debug"`, `"trace"`) to both the stdout and file layers immediately --
/// every log line from this point on is affected, no restart needed.
/// Rejects an unparseable directive instead of silently ignoring it.
pub fn set_level(level: &str) -> Result<(), String> {
    let handles = HANDLES.get().expect("log_control::init not called");
    let filter = EnvFilter::try_new(level).map_err(|error| error.to_string())?;
    handles.stdout.apply(filter)?;
    let filter = EnvFilter::try_new(level).map_err(|error| error.to_string())?;
    handles.file.apply(filter)
}
