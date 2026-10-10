use anyhow::{anyhow, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};
use uuid::Uuid;

use quinn::{ClientConfig, Connection, Endpoint, EndpointConfig, RecvStream, SendStream, ServerConfig, TokioRuntime};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::DigitallySignedStruct;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use warlay_protocol::{decode_relay_data, encode_relay_data, RelayControl};

const PUNCH_MAGIC: &[u8] = b"WARLAY_P2P_PUNCH_V1";

#[derive(Debug)]
pub struct P2pTunnelHandle {
    pub endpoint: Endpoint,
    pub connection: Connection,
    pub send: tokio::sync::Mutex<SendStream>,
    pub recv: tokio::sync::Mutex<RecvStream>,
    /// Runs through a helper relay instead of a direct path.
    pub relayed: bool,
    _shim: Option<RelayShim>,
}

#[derive(Debug)]
struct SkipServerVerification;

impl ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
        ]
    }
}

/// Without keep-alives an idle link (players sitting in the room before the
/// match starts) hits quinn's 30s idle timeout and the NAT drops its
/// mapping, so the link is dead by the time the game needs it.
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(4);
const MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(20);

fn transport_config() -> Arc<quinn::TransportConfig> {
    let mut transport = quinn::TransportConfig::default();
    transport.datagram_receive_buffer_size(Some(1200));
    transport.keep_alive_interval(Some(KEEP_ALIVE_INTERVAL));
    transport.max_idle_timeout(Some(
        quinn::IdleTimeout::try_from(MAX_IDLE_TIMEOUT).expect("idle timeout fits a QUIC varint"),
    ));
    Arc::new(transport)
}

fn configure_quic_server() -> Result<ServerConfig> {
    let cert = rcgen::generate_simple_self_signed(vec!["warlay-p2p".to_string()])?;
    let cert_der = CertificateDer::from(cert.cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()));

    let mut server_config = ServerConfig::with_single_cert(vec![cert_der], key_der)?;
    server_config.transport_config(transport_config());
    Ok(server_config)
}

fn configure_quic_client() -> Result<ClientConfig> {
    let crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();

    let mut client_config = ClientConfig::new(Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(crypto)?));
    client_config.transport_config(transport_config());
    Ok(client_config)
}

/// Realiza la perforación NAT (UDP punch burst) contra `peer_addr` usando `local_socket`
/// y establece una conexión QUIC. Si `act_as_quic_server` es true, actúa como servidor QUIC;
/// en caso contrario, actúa como cliente QUIC.
pub async fn punch_and_connect(
    local_socket: UdpSocket,
    peer_addr: SocketAddr,
    room_token: Uuid,
    act_as_quic_server: bool,
) -> Result<P2pTunnelHandle> {
    punch_and_connect_any(local_socket, &[peer_addr], room_token, act_as_quic_server).await
}

/// Same as `punch_and_connect`, but punches every address in `candidates`
/// at once (the peer's public address as the hub saw it, plus its LAN
/// addresses for players behind the same router) and continues with
/// whichever one the peer's punch actually arrives from.
pub async fn punch_and_connect_any(
    local_socket: UdpSocket,
    candidates: &[SocketAddr],
    room_token: Uuid,
    act_as_quic_server: bool,
) -> Result<P2pTunnelHandle> {
    info!(?candidates, %room_token, act_as_quic_server, "starting NAT hole punching burst");
    let mut punched_from = None;

    for _ in 0..5 {
        for candidate in candidates {
            let _ = local_socket.send_to(PUNCH_MAGIC, candidate).await;
        }
    }

    let mut buf = [0_u8; 1024];
    let punch_deadline = tokio::time::Instant::now() + Duration::from_millis(1500);

    while tokio::time::Instant::now() < punch_deadline {
        for candidate in candidates {
            let _ = local_socket.send_to(PUNCH_MAGIC, candidate).await;
        }

        if let Ok(Ok((len, src_addr))) = timeout(Duration::from_millis(150), local_socket.recv_from(&mut buf)).await {
            // The source port may differ from every candidate (the peer's
            // NAT picked a new one for this flow); the IP must still be one
            // the peer advertised.
            let known_ip = candidates.iter().any(|c| c.ip() == src_addr.ip());
            if known_ip && len >= PUNCH_MAGIC.len() && &buf[..PUNCH_MAGIC.len()] == PUNCH_MAGIC {
                for _ in 0..3 {
                    let _ = local_socket.send_to(PUNCH_MAGIC, src_addr).await;
                }
                punched_from = Some(src_addr);
                break;
            }
        }
    }

    let Some(peer_addr) = punched_from else {
        warn!(?candidates, %room_token, act_as_quic_server, "NAT hole punching failed to receive peer response");
        return Err(anyhow!("hole punching failed: no datagram received from peer"));
    };

    info!(%peer_addr, %room_token, act_as_quic_server, "NAT hole punching confirmed bidirectional UDP reachability");

    establish_quic(
        local_socket.into_std()?,
        peer_addr,
        room_token,
        act_as_quic_server,
        None,
    )
    .await
}

/// Runs the QUIC handshake over `std_socket` with `peer_addr` and opens
/// the primary stream. `shim` keeps a relay forwarder alive for as long
/// as the returned link lives.
async fn establish_quic(
    std_socket: std::net::UdpSocket,
    peer_addr: SocketAddr,
    room_token: Uuid,
    act_as_quic_server: bool,
    shim: Option<RelayShim>,
) -> Result<P2pTunnelHandle> {
    let relayed = shim.is_some();
    if act_as_quic_server {
        let server_config = configure_quic_server()?;
        let endpoint = Endpoint::new(
            EndpointConfig::default(),
            Some(server_config),
            std_socket,
            Arc::new(TokioRuntime),
        )?;

        info!(%room_token, relayed, "QUIC server endpoint listening, waiting for client connection");
        let incoming = endpoint
            .accept()
            .await
            .ok_or_else(|| anyhow!("QUIC endpoint closed before client connected"))?;

        let connection = incoming.await?;
        info!(%room_token, peer_addr = %connection.remote_address(), relayed, "QUIC connection established with peer");

        let (send, mut recv) = connection.accept_bi().await?;
        let mut init_buf = [0_u8; 1];
        recv.read_exact(&mut init_buf).await?;
        info!(%room_token, "QUIC bidirectional stream accepted from peer");

        Ok(P2pTunnelHandle {
            endpoint,
            connection,
            send: tokio::sync::Mutex::new(send),
            recv: tokio::sync::Mutex::new(recv),
            relayed,
            _shim: shim,
        })
    } else {
        let client_config = configure_quic_client()?;
        let endpoint = Endpoint::new(EndpointConfig::default(), None, std_socket, Arc::new(TokioRuntime))?;

        info!(%peer_addr, %room_token, relayed, "QUIC client connecting to peer endpoint");
        let connecting = endpoint.connect_with(client_config, peer_addr, "warlay-p2p")?;
        let connection = connecting.await?;

        info!(%room_token, peer_addr = %connection.remote_address(), relayed, "QUIC connection established with peer");

        let (mut send, recv) = connection.open_bi().await?;
        send.write_all(&[0x01]).await?;
        info!(%room_token, "QUIC bidirectional stream opened to peer");

        Ok(P2pTunnelHandle {
            endpoint,
            connection,
            send: tokio::sync::Mutex::new(send),
            recv: tokio::sync::Mutex::new(recv),
            relayed,
            _shim: shim,
        })
    }
}

/// Aborts the relay forwarder task when the link it serves is dropped.
#[derive(Debug)]
struct RelayShim(tokio::task::JoinHandle<()>);

impl Drop for RelayShim {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// How long to wait for the peer to bind its end of a relay channel.
const RELAY_BIND_TIMEOUT: Duration = Duration::from_secs(10);
const RELAY_REBIND_INTERVAL: Duration = Duration::from_secs(15);

/// Waits until both ends are bound on the helper; returns the channel id.
async fn bind_relay_channel(relay_socket: &UdpSocket, relay_addr: SocketAddr, bind: &[u8]) -> Result<u64> {
    let deadline = tokio::time::Instant::now() + RELAY_BIND_TIMEOUT;
    let mut buf = vec![0_u8; 2048];
    while tokio::time::Instant::now() < deadline {
        relay_socket.send_to(bind, relay_addr).await?;
        let wait_until = tokio::time::Instant::now() + Duration::from_millis(500);
        while let Ok(Ok((len, from))) = tokio::time::timeout_at(wait_until, relay_socket.recv_from(&mut buf)).await {
            if from != relay_addr {
                continue;
            }
            match RelayControl::decode(&buf[..len]) {
                Some(RelayControl::Bound {
                    channel,
                    peer_ready: true,
                }) => return Ok(channel),
                Some(RelayControl::Refused { reason }) => return Err(anyhow!("relay refused the bind: {reason}")),
                _ => {}
            }
        }
    }
    Err(anyhow!("the peer never bound its end of the relay channel"))
}

/// Last resort when hole punching is impossible (symmetric NAT): both
/// players bind a channel on the room's helper relay with their ticket, and
/// the QUIC session runs over it end to end -- the helper only forwards
/// encrypted datagrams.
///
/// quinn wants a plain UDP socket, so it gets a loopback one, and a small
/// forwarder ("shim") on another loopback socket wraps/unwraps the relay
/// framing; to quinn the peer simply lives at the shim's address.
pub async fn connect_via_relay(
    relay_addr: SocketAddr,
    ticket: &[u8],
    peer_id: Uuid,
    room_token: Uuid,
    act_as_quic_server: bool,
) -> Result<P2pTunnelHandle> {
    let relay_socket = UdpSocket::bind(if relay_addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).await?;
    let bind = RelayControl::Bind {
        ticket: ticket.to_vec(),
        peer_id,
    }
    .encode()?;

    info!(%relay_addr, %peer_id, %room_token, "binding a relay channel on the helper");
    let channel = bind_relay_channel(&relay_socket, relay_addr, &bind).await?;
    info!(%relay_addr, channel, %room_token, "relay channel ready");

    let quic_socket = std::net::UdpSocket::bind("127.0.0.1:0")?;
    let quic_addr = quic_socket.local_addr()?;
    let shim_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let shim_addr = shim_socket.local_addr()?;

    let forwarder = tokio::spawn(async move {
        let mut from_quic = vec![0_u8; 65_536];
        let mut from_relay = vec![0_u8; 65_536];
        let mut rebind = tokio::time::interval(RELAY_REBIND_INTERVAL);
        rebind.tick().await;
        loop {
            tokio::select! {
                res = shim_socket.recv_from(&mut from_quic) => {
                    let Ok((len, src)) = res else { break };
                    if src == quic_addr {
                        let framed = encode_relay_data(channel, &from_quic[..len]);
                        let _ = relay_socket.send_to(&framed, relay_addr).await;
                    }
                }
                res = relay_socket.recv_from(&mut from_relay) => {
                    let Ok((len, src)) = res else { break };
                    if src != relay_addr {
                        continue;
                    }
                    if let Some((ch, payload)) = decode_relay_data(&from_relay[..len]) {
                        if ch == channel {
                            let _ = shim_socket.send_to(payload, quic_addr).await;
                        }
                    }
                }
                // Keeps the channel and our NAT mapping toward the helper alive.
                _ = rebind.tick() => {
                    let _ = relay_socket.send_to(&bind, relay_addr).await;
                }
            }
        }
    });

    establish_quic(
        quic_socket,
        shim_addr,
        room_token,
        act_as_quic_server,
        Some(RelayShim(forwarder)),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket(secret: &[u8], room_id: Uuid, user_id: Uuid) -> Vec<u8> {
        let t = warlay_protocol::RoomTicket {
            room_id,
            user_id,
            is_host: false,
            expires_unix: u64::MAX,
        };
        warlay_crypto::sign_token(secret, &t.encode().unwrap())
    }

    #[tokio::test]
    async fn quic_link_through_the_helper_relay() {
        let secret = b"relay-test-secret".to_vec();
        let relay_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let relay_addr = relay_socket.local_addr().unwrap();
        Arc::new(warlay_core::UdpRelay::new(relay_socket, secret.clone())).spawn();

        let room = Uuid::new_v4();
        let (host_id, joiner_id) = (Uuid::new_v4(), Uuid::new_v4());
        let host_ticket = ticket(&secret, room, host_id);
        let joiner_ticket = ticket(&secret, room, joiner_id);

        let host =
            tokio::spawn(async move { connect_via_relay(relay_addr, &host_ticket, joiner_id, room, true).await });
        let joiner =
            tokio::spawn(async move { connect_via_relay(relay_addr, &joiner_ticket, host_id, room, false).await });
        let host = host.await.unwrap().expect("host relay link");
        let joiner = joiner.await.unwrap().expect("joiner relay link");
        assert!(host.relayed && joiner.relayed);

        joiner.send.lock().await.write_all(b"via relay").await.unwrap();
        let mut buf = [0_u8; 32];
        let n = host.recv.lock().await.read(&mut buf).await.unwrap().unwrap();
        assert_eq!(&buf[..n], b"via relay");
    }

    #[tokio::test]
    async fn p2p_quic_tunnel_mechanism_round_trip() {
        let socket1 = UdpSocket::bind("127.0.0.1:0").await.expect("bind socket1");
        let addr1 = socket1.local_addr().expect("addr1");
        let socket2 = UdpSocket::bind("127.0.0.1:0").await.expect("bind socket2");
        let addr2 = socket2.local_addr().expect("addr2");

        let room_token = Uuid::new_v4();

        let server_task = tokio::spawn(punch_and_connect(socket1, addr2, room_token, true));

        let client_task = tokio::spawn(punch_and_connect(socket2, addr1, room_token, false));

        let (server_res, client_res) = tokio::join!(server_task, client_task);

        let host_handle = server_res
            .expect("server task panicked")
            .expect("server P2P tunnel failed");
        let joiner_handle = client_res
            .expect("client task panicked")
            .expect("client P2P tunnel failed");

        // Test sending data from joiner to host over QUIC stream
        joiner_handle
            .send
            .lock()
            .await
            .write_all(b"hello from joiner over quic")
            .await
            .expect("joiner write");

        let mut host_buf = [0_u8; 64];
        let n_host = host_handle
            .recv
            .lock()
            .await
            .read(&mut host_buf)
            .await
            .expect("host read")
            .expect("host got bytes");
        assert_eq!(&host_buf[..n_host], b"hello from joiner over quic");

        // Test sending data from host to joiner over QUIC stream
        host_handle
            .send
            .lock()
            .await
            .write_all(b"hello from host over quic")
            .await
            .expect("host write");

        let mut joiner_buf = [0_u8; 64];
        let n_joiner = joiner_handle
            .recv
            .lock()
            .await
            .read(&mut joiner_buf)
            .await
            .expect("joiner read")
            .expect("joiner got bytes");
        assert_eq!(&joiner_buf[..n_joiner], b"hello from host over quic");
    }
}
