use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use warlay_core::LoopGuard;
use warlay_protocol::{PacketOrigin, RelayEnvelope};
use warlay_transport::UdpTransport;

/// Client-side counterpart to the server's lan-mesh relay engine: makes this
/// desktop machine act as one more mesh peer. Broadcast game traffic
/// captured on this machine's local subnet is tunneled out to the selected
/// remote bridge(s); traffic tunneled back in is re-injected as a local
/// broadcast so the locally running game sees it as an ordinary LAN
/// announcement.
pub struct RelayClient {
    transport: Arc<UdpTransport>,
    peers: Vec<SocketAddr>,
    loop_guard: LoopGuard,
    broadcast_port: u16,
}

/// Owns the background tasks driving a running relay client. Dropping (or
/// calling `abort`) stops relaying immediately.
pub struct RelayClientHandle {
    task: JoinHandle<()>,
}

impl RelayClientHandle {
    pub fn abort(self) {
        self.task.abort();
    }
}

impl RelayClient {
    pub fn new(transport: Arc<UdpTransport>, peers: Vec<SocketAddr>, loop_guard_ttl: Duration) -> Self {
        let broadcast_port = transport.local_addr().port();
        Self {
            transport,
            peers,
            loop_guard: LoopGuard::new(loop_guard_ttl),
            broadcast_port,
        }
    }

    pub fn spawn(self: Arc<Self>) -> RelayClientHandle {
        let task = tokio::spawn(self.run());
        RelayClientHandle { task }
    }

    async fn run(self: Arc<Self>) {
        let mut buf = vec![0_u8; 65_535];
        loop {
            let (len, src) = match self.transport.recv_from(&mut buf).await {
                Ok(result) => result,
                Err(error) => {
                    warn!(%error, "failed to receive datagram on local relay socket");
                    continue;
                }
            };
            let payload = &buf[..len];

            if self.peers.contains(&src) {
                self.handle_inbound_from_peer(src, payload).await;
            } else {
                self.handle_local_traffic(src, payload).await;
            }
        }
    }

    async fn handle_local_traffic(&self, src: SocketAddr, payload: &[u8]) {
        if self.loop_guard.is_echo(payload) {
            debug!(%src, "dropping self-echoed broadcast to prevent a relay loop");
            return;
        }
        if self.peers.is_empty() {
            return;
        }

        let envelope = RelayEnvelope::new(src, PacketOrigin::Broadcast, payload.to_vec());
        let encoded = match envelope.encode() {
            Ok(bytes) => bytes,
            Err(error) => {
                warn!(%error, "failed to encode relay envelope");
                return;
            }
        };

        for peer in &self.peers {
            if let Err(error) = self.transport.send_to(&encoded, *peer).await {
                warn!(%error, %peer, "failed to forward datagram to peer");
            }
        }
    }

    async fn handle_inbound_from_peer(&self, peer_addr: SocketAddr, payload: &[u8]) {
        let envelope = match RelayEnvelope::decode(payload) {
            Ok(envelope) => envelope,
            Err(error) => {
                warn!(%error, peer = %peer_addr, "failed to decode relay envelope from peer");
                return;
            }
        };

        self.loop_guard.mark(&envelope.payload);

        let destination = match envelope.origin_kind {
            PacketOrigin::Broadcast => UdpTransport::broadcast_addr(self.broadcast_port),
            PacketOrigin::Unicast => envelope.origin_addr,
        };

        if let Err(error) = self.transport.send_to(&envelope.payload, destination).await {
            warn!(%error, %destination, "failed to inject relayed datagram into local subnet");
            return;
        }
        info!(
            origin = %envelope.origin_addr,
            via_peer = %peer_addr,
            bytes = envelope.payload.len(),
            "injected relayed broadcast into local subnet"
        );
    }
}
