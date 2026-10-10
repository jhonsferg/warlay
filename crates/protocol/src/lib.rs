mod control;
mod control_ws;
mod envelope;
mod error;
mod handshake;
mod hub;
mod rendezvous;
mod tunnel;

/// LAN-mesh mode: UDP control-plane messages (discovery / game listing
/// between `warlay-server lan-mesh` peers on isolated subnets).
pub use control::{
    AnnounceResponse, ControlMessage, DiscoverRequest, GamesListResponse, ListGamesRequest, ObservedGame,
};
/// LAN-mesh mode: the UDP data-plane envelope reflected between peers.
pub use envelope::{PacketOrigin, RelayEnvelope};

/// Hub mode: the WebSocket matchmaking control-plane protocol between a
/// client and the centralized VPS server.
pub use control_ws::{ClientMessage, RoomMember, RoomStatus, RoomSummary, ServerMessage};
/// Hub mode: the plaintext connection-opening handshake and the encrypted
/// authentication exchange that follows it.
pub use handshake::{AuthRequest, AuthResponse, HandshakeHello, HandshakeWelcome};
/// Hub mode: the UDP data-plane frame, scoped to a room by `room_token`
/// rather than by static peer address.
pub use hub::RelayFrame;
/// Hub mode: the UDP rendezvous protocol for P2P direct tunnel establishment.
pub use rendezvous::{PeerInfo, RendezvousMessage};
/// Hub mode: the TCP tunnel handshake carrying the real WC3 game session.
pub use tunnel::TunnelHello;

pub use error::ProtocolError;

/// Magic prefix identifying a LAN-mesh control-plane frame, distinguishing it
/// from raw game traffic that may traverse the same subnet.
pub const PROTOCOL_MAGIC: [u8; 4] = *b"WLB1";
pub const PROTOCOL_VERSION: u16 = 1;
pub mod ipc;
