use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("transport error: {0}")]
    Transport(#[from] warlay_transport::TransportError),
    #[error("protocol error: {0}")]
    Protocol(#[from] warlay_protocol::ProtocolError),
}
