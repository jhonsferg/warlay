use std::net::SocketAddr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("failed to bind udp socket on {addr}: {source}")]
    Bind {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to configure socket options: {0}")]
    Configure(#[source] std::io::Error),
    #[error("io error during socket operation: {0}")]
    Io(#[source] std::io::Error),
}
