use crate::{GameObservationStore, PeerRegistry};
use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use warlay_protocol::{AnnounceResponse, ControlMessage, GamesListResponse, ObservedGame, PROTOCOL_VERSION};
use warlay_transport::UdpTransport;

/// Answers control-plane queries from Launcher clients: `Discover` (are you
/// there, and what's your name) and `ListGames` (what have you seen on your
/// local subnet recently).
pub struct ControlResponder {
    transport: Arc<UdpTransport>,
    peers: Arc<PeerRegistry>,
    observations: Arc<GameObservationStore>,
    server_name: String,
    relay_port: u16,
    started_at: Instant,
}

impl ControlResponder {
    pub fn new(
        transport: Arc<UdpTransport>,
        peers: Arc<PeerRegistry>,
        observations: Arc<GameObservationStore>,
        server_name: impl Into<String>,
        relay_port: u16,
    ) -> Self {
        Self {
            transport,
            peers,
            observations,
            server_name: server_name.into(),
            relay_port,
            started_at: Instant::now(),
        }
    }

    pub fn spawn(self: Arc<Self>) -> JoinHandle<()> {
        tokio::spawn(self.run())
    }

    async fn run(self: Arc<Self>) {
        let mut buf = vec![0_u8; 4096];
        loop {
            let (len, src) = match self.transport.recv_from(&mut buf).await {
                Ok(result) => result,
                Err(error) => {
                    warn!(%error, "failed to receive datagram on control socket");
                    continue;
                }
            };

            let message = match ControlMessage::decode(&buf[..len]) {
                Ok(message) => message,
                Err(error) => {
                    debug!(%error, %src, "ignoring malformed control message");
                    continue;
                }
            };

            let response = match message {
                ControlMessage::Discover(_) => Some(self.build_announce()),
                ControlMessage::ListGames(_) => Some(self.build_games_list()),
                ControlMessage::Announce(_) | ControlMessage::GamesList(_) => None,
            };

            let Some(response) = response else { continue };

            match response.encode() {
                Ok(bytes) => {
                    if let Err(error) = self.transport.send_to(&bytes, src).await {
                        warn!(%error, %src, "failed to send control response");
                    }
                }
                Err(error) => warn!(%error, "failed to encode control response"),
            }
        }
    }

    fn build_announce(&self) -> ControlMessage {
        ControlMessage::Announce(AnnounceResponse {
            server_name: self.server_name.clone(),
            protocol_version: PROTOCOL_VERSION,
            relay_port: self.relay_port,
            known_peers: self.peers.len() as u32,
            uptime_secs: self.started_at.elapsed().as_secs(),
        })
    }

    fn build_games_list(&self) -> ControlMessage {
        let now = Instant::now();
        let games = self
            .observations
            .snapshot()
            .into_iter()
            .map(|entry| ObservedGame {
                source_addr: entry.source_addr.to_string(),
                game_name: entry.extract_readable_name(),
                last_seen_secs_ago: now.duration_since(entry.last_seen).as_secs(),
                payload_len: entry.payload_sample.len(),
            })
            .collect();
        ControlMessage::GamesList(GamesListResponse { games })
    }
}
