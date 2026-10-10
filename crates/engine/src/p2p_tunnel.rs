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

const PUNCH_MAGIC: &[u8] = b"WARLAY_P2P_PUNCH_V1";

#[derive(Debug)]
pub struct P2pTunnelHandle {
    pub endpoint: Endpoint,
    pub connection: Connection,
    pub send: tokio::sync::Mutex<SendStream>,
    pub recv: tokio::sync::Mutex<RecvStream>,
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

fn configure_quic_server() -> Result<ServerConfig> {
    let cert = rcgen::generate_simple_self_signed(vec!["warlay-p2p".to_string()])?;
    let cert_der = CertificateDer::from(cert.cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()));

    let mut server_config = ServerConfig::with_single_cert(vec![cert_der], key_der)?;
    let mut transport = quinn::TransportConfig::default();
    transport.datagram_receive_buffer_size(Some(1200));
    server_config.transport_config(Arc::new(transport));
    Ok(server_config)
}

fn configure_quic_client() -> Result<ClientConfig> {
    let crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();

    let mut client_config = ClientConfig::new(Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(crypto)?));
    let mut transport = quinn::TransportConfig::default();
    transport.datagram_receive_buffer_size(Some(1200));
    client_config.transport_config(Arc::new(transport));
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
    info!(%peer_addr, %room_token, act_as_quic_server, "starting NAT hole punching burst");
    let mut punched = false;

    for _ in 0..5 {
        let _ = local_socket.send_to(PUNCH_MAGIC, peer_addr).await;
    }

    let mut buf = [0_u8; 1024];
    let punch_deadline = tokio::time::Instant::now() + Duration::from_millis(1500);

    while tokio::time::Instant::now() < punch_deadline {
        let remaining = punch_deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }

        let _ = local_socket.send_to(PUNCH_MAGIC, peer_addr).await;

        match timeout(Duration::from_millis(150), local_socket.recv_from(&mut buf)).await {
            Ok(Ok((len, _src_addr))) => {
                if len >= PUNCH_MAGIC.len() && &buf[..PUNCH_MAGIC.len()] == PUNCH_MAGIC {
                    punched = true;
                    for _ in 0..3 {
                        let _ = local_socket.send_to(PUNCH_MAGIC, peer_addr).await;
                    }
                    break;
                }
            }
            _ => {}
        }
    }

    if !punched {
        warn!(%peer_addr, %room_token, act_as_quic_server, "NAT hole punching failed to receive peer response");
        return Err(anyhow!("hole punching failed: no datagram received from peer"));
    }

    info!(%peer_addr, %room_token, act_as_quic_server, "NAT hole punching confirmed bidirectional UDP reachability");

    let std_socket = local_socket.into_std()?;

    if act_as_quic_server {
        let server_config = configure_quic_server()?;
        let endpoint = Endpoint::new(
            EndpointConfig::default(),
            Some(server_config),
            std_socket,
            Arc::new(TokioRuntime),
        )?;

        info!(%room_token, "QUIC server endpoint listening, waiting for client connection");
        let incoming = endpoint
            .accept()
            .await
            .ok_or_else(|| anyhow!("QUIC endpoint closed before client connected"))?;

        let connection = incoming.await?;
        info!(%room_token, peer_addr = %connection.remote_address(), "QUIC connection established with peer");

        let (send, mut recv) = connection.accept_bi().await?;
        let mut init_buf = [0_u8; 1];
        recv.read_exact(&mut init_buf).await?;
        info!(%room_token, "QUIC bidirectional stream accepted from peer");

        Ok(P2pTunnelHandle {
            endpoint,
            connection,
            send: tokio::sync::Mutex::new(send),
            recv: tokio::sync::Mutex::new(recv),
        })
    } else {
        let client_config = configure_quic_client()?;
        let endpoint = Endpoint::new(EndpointConfig::default(), None, std_socket, Arc::new(TokioRuntime))?;

        info!(%peer_addr, %room_token, "QUIC client connecting to peer endpoint");
        let connecting = endpoint.connect_with(client_config, peer_addr, "warlay-p2p")?;
        let connection = connecting.await?;

        info!(%room_token, peer_addr = %connection.remote_address(), "QUIC connection established with peer");

        let (mut send, recv) = connection.open_bi().await?;
        send.write_all(&[0x01]).await?;
        info!(%room_token, "QUIC bidirectional stream opened to peer");

        Ok(P2pTunnelHandle {
            endpoint,
            connection,
            send: tokio::sync::Mutex::new(send),
            recv: tokio::sync::Mutex::new(recv),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
