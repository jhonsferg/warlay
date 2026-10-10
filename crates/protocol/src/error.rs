use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("failed to serialize message: {0}")]
    Serialize(#[source] bincode::Error),
    #[error("failed to deserialize message: {0}")]
    Deserialize(#[source] bincode::Error),
    #[error("frame is too short or missing the protocol magic prefix")]
    InvalidMagic,
    #[error("failed to serialize control message to JSON: {0}")]
    JsonSerialize(#[source] serde_json::Error),
    #[error("failed to deserialize control message from JSON: {0}")]
    JsonDeserialize(#[source] serde_json::Error),
}
