use std::io::Write;
use tracing_subscriber::prelude::*;

/// `std::fs::File`'s `Write::flush` is a documented no-op on every
/// platform -- `write_all` already goes straight through `WriteFile`, so
/// there's nothing buffered at the Rust level to flush. That's fine for
/// getting bytes into the OS, but on Windows another process reading this
/// same file (e.g. someone tailing the log live while the daemon runs)
/// sees a stale/lagging file size for as long as this handle stays open --
/// NTFS updates the visible end-of-file for *other* openers lazily, not on
/// every write. `tracing_appender`'s worker thread calls `flush()` after
/// every batch of log lines it drains, so routing that through
/// `sync_data()` (Windows' `FlushFileBuffers`) instead keeps the file's
/// on-disk size honest for live tailing, not just after this process's
/// handle finally closes (confirmed live logs were invisible until the
/// daemon was killed -- this is why).
struct SyncOnFlush(std::fs::File);

impl Write for SyncOnFlush {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.sync_data()
    }
}

/// Same directory the desktop app logs to (see `desktop/src-tauri/src/log_cleanup.rs`'s
/// `log_dir` -- hardcoded independently here rather than shared, since the
/// daemon doesn't otherwise depend on desktop's crate) -- putting the
/// daemon's own logs alongside the desktop app's means the existing
/// retention sweep (`log_cleanup`) already cleans these up too, for free.
fn log_dir() -> std::path::PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    std::path::Path::new(&appdata)
        .join("com.warlay.lanlauncher")
        .join("logs")
}

/// A "wnetd-" prefix keeps these visually and lexically separate from the
/// desktop app's "warlay-*.log" files in the same directory, while still
/// being one file per process launch for the same reason desktop's are.
fn log_file_name() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "wnetd-{:04}{:02}{:02}-{:02}{:02}{:02}.log",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// The daemon runs with no console window in release builds (see the
/// `windows_subsystem` attribute on `main.rs`), so `tracing`'s default
/// stdout writer had nowhere to go and every log line -- including whatever
/// explains a crash -- was silently lost. This adds a same-shaped file
/// output, and a panic hook that appends directly to that file with a plain
/// synchronous write (not routed through `tracing`'s own possibly-buffered
/// layer) so a panic message is captured even if the process exits
/// immediately after, on any thread, not just ones tokio manages.
///
/// `initial_level` seeds both layers (from `AgentStore`, read by the
/// caller before this runs -- same ordering constraint desktop's own
/// `ClientStore`/subscriber setup has). `RUST_LOG`, if set, still wins over
/// it, matching desktop's own precedence. Both layers are wrapped in a
/// `reload::Layer` so `crate::log_control::set_level` can change the level
/// live afterwards, no restart needed -- see that module's own docs.
pub fn init_logging(initial_level: &str) {
    let dir = log_dir();
    let _ = std::fs::create_dir_all(&dir);
    let log_path = dir.join(log_file_name());

    let env_override = std::env::var("RUST_LOG").ok();
    let initial = env_override.unwrap_or_else(|| initial_level.to_string());

    let (stdout_filter, stdout_handle) =
        tracing_subscriber::reload::Layer::new(tracing_subscriber::EnvFilter::new(&initial));

    if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
        let (file_writer, guard) = tracing_appender::non_blocking(SyncOnFlush(file));
        // Leaked deliberately: this lives for the whole process, same
        // reasoning as desktop's own equivalent guard in lib.rs.
        Box::leak(Box::new(guard));
        let (file_filter, file_handle) =
            tracing_subscriber::reload::Layer::new(tracing_subscriber::EnvFilter::new(&initial));
        // Two separate layers, not one shared writer combined via `.and()`
        // -- a single `fmt()` layer formats each event once and sends the
        // identical bytes to every writer it's given, so ANSI color codes
        // (on by default) ended up baked into the .log file's text too,
        // where nothing ever renders them -- just escape-code noise. Same
        // fix, same reasoning, as desktop's own equivalent split in
        // desktop-tauri/src/lib.rs.
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .with_writer(std::io::stdout)
                    .with_filter(stdout_filter),
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .with_writer(file_writer)
                    .with_ansi(false)
                    .with_filter(file_filter),
            )
            .init();
        crate::log_control::init(stdout_handle.into(), file_handle.into());
    } else {
        // Couldn't open the log file (e.g. the directory itself is
        // inaccessible) -- fall back to stdout-only rather than not
        // logging at all. There's no file layer to hand `log_control` a
        // real handle for, so the same stdout handle fills both slots --
        // harmless (it just reloads the one real layer twice) and keeps
        // `set_level` from panicking on a missing handle in this fallback.
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .with_writer(std::io::stdout)
                    .with_filter(stdout_filter),
            )
            .init();
        crate::log_control::init(stdout_handle.clone().into(), stdout_handle.into());
    }

    install_panic_hook(log_path);
}

fn install_panic_hook(log_path: std::path::PathBuf) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        let now = time::OffsetDateTime::now_utc();
        let line = format!(
            "{now} PANIC ({:?}): {info}\n",
            std::thread::current().name().unwrap_or("<unnamed>")
        );
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
            let _ = f.write_all(line.as_bytes());
        }
        default_hook(info);
    }));
}
