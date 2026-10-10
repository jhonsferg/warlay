mod process_launcher;

pub use process_launcher::{close_job, is_process_running_at, job_process_ids, launch, GameJob};

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LauncherError {
    #[error("executable not found at {0}")]
    NotFound(PathBuf),
    #[error("failed to launch {path}: {source}")]
    Spawn {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
