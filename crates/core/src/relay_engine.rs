use crate::{GameObservationStore, LoopGuard, PeerRegistry};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use warlay_protocol::{PacketOrigin, RelayEnvelope};
use warlay_transport::UdpTransport;

/// The data-plane heart of the bridge: reflects locally captured broadcast
/// traffic out to every configured peer, and re-injects traffic arriving
/// from peers back into the local subnet as a broadcast.
///
/// Topology is a star/mesh where each node only ever relays what it captured
/// itself; inbound peer traffic is never re-forwarded to other peers, which
/// keeps an N-node mesh loop-free without needing hop counts or TTLs.
pub struct RelayEngine {
    transport: Arc<UdpTransport>,
    peers: Arc<PeerRegistry>,
    observations: Arc<GameObservationStore>,
    loop_guard: LoopGuard,
    broadcast_port: u16,
}

pub struct RelayEngineHandle {
    pub data_task: JoinHandle<()>,
    pub cleanup_task: JoinHandle<()>,
}

impl RelayEngine {
    pub fn new(
        transport: Arc<UdpTransport>,
        peers: Arc<PeerRegistry>,
        observations: Arc<GameObservationStore>,
        loop_guard_ttl: Duration,
    ) -> Self {
        let broadcast_port = transport.local_addr().port();
        Self {
            transport,
            peers,
            observations,
            loop_guard: LoopGuard::new(loop_guard_ttl),
            broadcast_port,
        }
    }

    pub fn spawn(self: Arc<Self>, observation_ttl: Duration) -> RelayEngineHandle {
        let data_task = tokio::spawn(self.clone().run_data_loop());
        let cleanup_task = tokio::spawn(self.run_cleanup_loop(observation_ttl));
        RelayEngineHandle {
            data_task,
            cleanup_task,
        }
    }

    async fn run_data_loop(self: Arc<Self>) {
        let mut buf = vec![0_u8; 65_535];
        loop {
            let (len, src) = match self.transport.recv_from(&mut buf).await {
                Ok(result) => result,
                Err(error) => {
                    warn!(%error, "failed to receive datagram on relay socket");
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

        self.observations.record(src, payload);

        let peers = self.peers.snapshot();
        if peers.is_empty() {
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

        for peer in peers {
            if let Err(error) = self.transport.send_to(&encoded, peer.addr).await {
                warn!(%error, peer = %peer.addr, "failed to forward datagram to peer");
            }
        }
        debug!(%src, bytes = payload.len(), "forwarded local broadcast to peer mesh");
    }

    async fn handle_inbound_from_peer(&self, peer_addr: SocketAddr, payload: &[u8]) {
        let envelope = match RelayEnvelope::decode(payload) {
            Ok(envelope) => envelope,
            Err(error) => {
                warn!(%error, peer = %peer_addr, "failed to decode relay envelope from peer");
                return;
            }
        };

        self.observations.record(envelope.origin_addr, &envelope.payload);
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

    async fn run_cleanup_loop(self: Arc<Self>, ttl: Duration) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            self.observations.prune_expired(ttl);
        }
    }
}
