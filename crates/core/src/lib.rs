mod control_responder;
mod error;
mod loop_guard;
mod observation;
mod peer;
mod relay_engine;
mod relay_hub;
mod tcp_tunnel_hub;
mod udp_relay;
pub mod udp_rendezvous;

/// LAN-mesh mode.
pub use control_responder::ControlResponder;
pub use loop_guard::LoopGuard;
pub use observation::{GameObservationStore, ObservedEntry};
pub use peer::{PeerHandle, PeerRegistry};
pub use relay_engine::{RelayEngine, RelayEngineHandle};

/// Hub mode.
pub use relay_hub::{RelayHub, RelayHubHandle};
pub use tcp_tunnel_hub::{TcpTunnelHub, TcpTunnelHubHandle};
/// Helper servers.
pub use udp_relay::{UdpRelay, UdpRelayHandle};
pub use udp_rendezvous::{verify_ticket, UdpRendezvous, UdpRendezvousHandle};

pub use error::CoreError;
