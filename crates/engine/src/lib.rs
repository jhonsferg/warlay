mod discovery_service;
mod hub_address;
// LAN capture is per platform (raw SIO_RCVALL socket on Windows, AF_PACKET
// on Linux) behind one API; everything else builds anywhere.
#[cfg(windows)]
mod lan_capture;
#[cfg(target_os = "linux")]
#[path = "lan_capture_linux.rs"]
mod lan_capture;
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
mod lan_filter;
mod lan_rebroadcast;
mod matchmaking_client;
mod p2p_mesh;
mod p2p_tunnel;
mod relay_client;
mod subnet;
mod tcp_tunnel_client;
mod tls;
mod w3gs_sniff;

pub use discovery_service::{query_games, spawn_discovery, RelayInfo};
pub use hub_address::HubAddress;
#[cfg(any(windows, target_os = "linux"))]
pub use lan_capture::{spawn as spawn_lan_capture, LanCaptureHandle};
pub use lan_rebroadcast::{spawn as spawn_lan_rebroadcast, LanRebroadcastHandle};
pub use matchmaking_client::MatchmakingConnection;
pub use p2p_mesh::{MeshConfig, PathToHost, PeerLinkState, PeerMesh, PeerMeshHandle};
pub use p2p_tunnel::{connect_via_relay, punch_and_connect, P2pTunnelHandle};
pub use relay_client::{RelayClient, RelayClientHandle};
pub use subnet::{list_local_subnets, DetectedSubnet, Ipv4Cidr};
pub use tcp_tunnel_client::{
    start_hosting, start_hosting_mesh, start_hosting_with_p2p, start_joining, start_joining_mesh,
    start_joining_with_p2p, HubTunnelEndpoint, TunnelHandle,
};
pub use w3gs_sniff::contains_create_game;
pub use warlay_core::LoopGuard;

use thiserror::Error;
use warlay_transport::TransportError;

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
    Protocol(#[from] warlay_protocol::ProtocolError),
    #[error("failed to enumerate local network interfaces: {0}")]
    InterfaceEnumeration(#[source] std::io::Error),
    #[error("relay did not respond within the configured timeout")]
    Timeout,
    #[error("websocket connection failed: {0}")]
    WebSocket(String),
    #[error("'{0}' is not a valid hub address (expected host:port, ws://host or wss://host)")]
    InvalidHubAddress(String),
    #[error("not connected to a matchmaking server")]
    NotConnected,
    #[error("tunnel error: {0}")]
    Tunnel(#[from] tcp_tunnel_client::TunnelError),
    #[error("'{server_name}' requires a password")]
    PasswordRequired { server_name: String },
    #[error("{message}")]
    AuthRejected { message: String, retry_as_new: bool },
}
