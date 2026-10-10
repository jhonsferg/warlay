use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Any(#[from] anyhow::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Core(#[from] warlay::core::CoreError),
    #[error(transparent)]
    Transport(#[from] warlay::transport::TransportError),

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Launcher(#[from] crate::launcher::LauncherError),
    #[error(transparent)]
    Network(#[from] crate::network::NetworkError),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Not implemented")]
    NotImplemented,
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
