mod discovery_service;
mod relay_client;
mod subnet;

pub use discovery_service::{query_games, spawn_discovery, RelayInfo};
pub use relay_client::{RelayClient, RelayClientHandle};
pub use subnet::{list_local_subnets, DetectedSubnet, Ipv4Cidr};

use crate::transport::TransportError;
use thiserror::Error;

/// Scanning a range larger than this is almost always a typo (e.g. a /8
/// instead of a /24) and would flood the network with probe packets.
pub const MAX_SCAN_HOSTS: u64 = 4096;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("'{0}' is not a valid IPv4 CIDR range (expected e.g. 192.168.1.0/24)")]
    InvalidCidr(String),
    #[error("requested scan covers {0} hosts, which exceeds the {MAX_SCAN_HOSTS} host safety limit")]
    RangeTooLarge(u64),
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("protocol error: {0}")]
    Protocol(#[from] crate::protocol::ProtocolError),
    #[error("failed to enumerate local network interfaces: {0}")]
    InterfaceEnumeration(#[source] std::io::Error),
    #[error("relay did not respond within the configured timeout")]
    Timeout,
    #[error("websocket connection failed: {0}")]
    WebSocket(String),
    #[error("not connected to a matchmaking server")]
    NotConnected,
    #[error("'{server_name}' requires a password")]
    PasswordRequired { server_name: String },
    #[error("{message}")]
    AuthRejected { message: String, retry_as_new: bool },
}
