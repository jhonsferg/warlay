use super::subnet::Ipv4Cidr;
use super::NetworkError;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, warn};
use warlay_protocol::{
    ControlMessage, DiscoverRequest, GamesListResponse, ListGamesRequest, ObservedGame, PROTOCOL_VERSION,
};
use warlay_transport::UdpTransport;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayInfo {
    pub addr: SocketAddr,
    pub server_name: String,
    pub protocol_version: u16,
    pub relay_port: u16,
    pub known_peers: u32,
    pub uptime_secs: u64,
}

/// Sends a `Discover` probe to every host in the given CIDR ranges from a
/// single shared socket and streams back each `Announce` reply as it
/// arrives, closing the channel once `timeout` has elapsed since the scan
/// started. Streaming (rather than collecting into a `Vec` up front) lets
/// the caller surface results to the UI in real time.
pub fn spawn_discovery(cidr_ranges: Vec<Ipv4Cidr>, control_port: u16, timeout: Duration) -> mpsc::Receiver<RelayInfo> {
    let (tx, rx) = mpsc::channel(128);

    tokio::spawn(async move {
        let transport = match UdpTransport::bind(
            std::net::SocketAddr::V4(std::net::SocketAddrV4::new(std::net::Ipv4Addr::UNSPECIFIED, 0)),
            false,
        )
        .await
        {
            Ok(transport) => transport,
            Err(error) => {
                warn!(%error, "failed to bind discovery probe socket");
                return;
            }
        };

        let request = ControlMessage::Discover(DiscoverRequest {
            client_id: "warlay-launcher".to_string(),
            protocol_version: PROTOCOL_VERSION,
        });
        let encoded = match request.encode() {
            Ok(bytes) => bytes,
            Err(error) => {
                warn!(%error, "failed to encode discovery request");
                return;
            }
        };

        let mut candidate_count = 0usize;
        for cidr in &cidr_ranges {
            for host in cidr.hosts() {
                let target = SocketAddr::new(std::net::IpAddr::V4(host), control_port);
                if let Err(error) = transport.send_to(&encoded, target).await {
                    debug!(%error, %target, "failed to send discovery probe");
                    continue;
                }
                candidate_count += 1;
            }
        }
        debug!(candidate_count, "discovery probes sent");

        let deadline = Instant::now() + timeout;
        let mut buf = vec![0_u8; 4096];
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }

            let received = tokio::time::timeout(remaining, transport.recv_from(&mut buf)).await;
            let Ok(Ok((len, src))) = received else {
                break;
            };

            let Ok(ControlMessage::Announce(announce)) = ControlMessage::decode(&buf[..len]) else {
                continue;
            };

            let info = RelayInfo {
                addr: src,
                server_name: announce.server_name,
                protocol_version: announce.protocol_version,
                relay_port: announce.relay_port,
                known_peers: announce.known_peers,
                uptime_secs: announce.uptime_secs,
            };

            if tx.send(info).await.is_err() {
                break;
            }
        }
    });

    rx
}

/// Queries a single, already-known relay for the games it has recently
/// observed on its local subnet.
pub async fn query_games(target: SocketAddr, timeout: Duration) -> Result<Vec<ObservedGame>, NetworkError> {
    let transport = UdpTransport::bind(
        std::net::SocketAddr::V4(std::net::SocketAddrV4::new(std::net::Ipv4Addr::UNSPECIFIED, 0)),
        false,
    )
    .await?;
    let request = ControlMessage::ListGames(ListGamesRequest);
    let encoded = request.encode()?;
    transport.send_to(&encoded, target).await?;

    let mut buf = vec![0_u8; 65_535];
    let (len, _) = tokio::time::timeout(timeout, transport.recv_from(&mut buf))
        .await
        .map_err(|_| NetworkError::Timeout)??;

    match ControlMessage::decode(&buf[..len])? {
        ControlMessage::GamesList(GamesListResponse { games }) => Ok(games),
        _ => Ok(Vec::new()),
    }
}
