use super::NetworkError;
use futures_util::{SinkExt, StreamExt};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, warn};
use uuid::Uuid;
use warlay_crypto::{EphemeralKeypair, Opener, Sealer};
use warlay_protocol::{AuthRequest, AuthResponse, ClientMessage, HandshakeHello, HandshakeWelcome, ServerMessage};

const PING_INTERVAL: Duration = Duration::from_secs(5);

pub struct ConnectOutcome {
    pub connection: MatchmakingConnection,
    pub server_id: Uuid,
    /// The server's own self-reported name, straight from
    /// `HandshakeWelcome` — never something the user typed in. Refreshed on
    /// every successful connect so a server rename is picked up silently.
    pub server_name: String,
    /// `Some` only when the server just issued a brand-new key (first-time
    /// or password auth) — the caller is responsible for persisting it.
    /// `None` on a successful `Proof` reauth, since the stored key is
    /// already correct.
    pub new_psk: Option<[u8; 32]>,
}

/// A persistent, encrypted WebSocket connection to a hub's matchmaking
/// control plane. Owns three background tasks (writer, reader, ping-loop)
/// and exposes a single `send` method; every inbound `ServerMessage` (other
/// than `Pong`, intercepted here to compute round-trip latency) is handed to
/// the `on_message` callback supplied at connect time — the command layer
/// wires that straight into a Tauri event emission, keeping this module
/// unaware that Tauri exists.
pub struct MatchmakingConnection {
    outbound: mpsc::UnboundedSender<ClientMessage>,
    reader_task: JoinHandle<()>,
    writer_task: JoinHandle<()>,
    ping_task: JoinHandle<()>,
}

impl MatchmakingConnection {
    /// Performs the full handshake (see `server/README.md` for the
    /// wire-level description this mirrors) and, on success, returns a live
    /// connection already past authentication.
    ///
    /// `stored_psk` is whatever this client previously saved for this
    /// server (`None` on first contact, or after the server purged it).
    /// `password` is only consulted when `stored_psk` is `None` — pass it
    /// only once the caller has confirmed (via a first attempt returning
    /// `NetworkError::PasswordRequired`) that the server actually needs one.
    pub async fn connect(
        ws_url: &str,
        client_id: Uuid,
        stored_psk: Option<[u8; 32]>,
        password: Option<String>,
        on_message: impl Fn(ServerMessage) + Send + Sync + 'static,
        on_ping: impl Fn(u64) + Send + Sync + 'static,
        on_lan_packet: impl Fn(Vec<u8>) + Send + Sync + 'static,
        on_disconnected: impl Fn() + Send + Sync + 'static,
    ) -> Result<ConnectOutcome, NetworkError> {
        // The connector only kicks in for `wss://` URLs; `ws://` stays plain.
        let connector = tokio_tungstenite::Connector::Rustls(super::tls::client_config());
        let (stream, _response) =
            tokio_tungstenite::connect_async_tls_with_config(ws_url, None, false, Some(connector))
                .await
                .map_err(|error| NetworkError::WebSocket(error.to_string()))?;
        let (mut ws_sink, mut ws_stream) = stream.split();

        let client_keypair = EphemeralKeypair::generate();
        let hello = HandshakeHello {
            client_id,
            client_ephemeral_pubkey: client_keypair.public_bytes,
        };
        ws_sink
            .send(WsMessage::Text(hello.to_json()?))
            .await
            .map_err(|error| NetworkError::WebSocket(error.to_string()))?;

        let welcome_frame = ws_stream
            .next()
            .await
            .ok_or_else(|| NetworkError::WebSocket("conexión cerrada durante el saludo".to_string()))?
            .map_err(|error| NetworkError::WebSocket(error.to_string()))?;
        let WsMessage::Text(welcome_text) = welcome_frame else {
            return Err(NetworkError::WebSocket(
                "se esperaba el saludo del servidor en texto plano".to_string(),
            ));
        };
        let welcome = HandshakeWelcome::from_json(&welcome_text)?;

        if welcome.requires_password && stored_psk.is_none() && password.is_none() {
            return Err(NetworkError::PasswordRequired {
                server_name: welcome.server_name,
            });
        }
        let server_name = welcome.server_name.clone();

        let keys = client_keypair
            .derive_session_keys(&welcome.server_ephemeral_pubkey)
            .map_err(|_| NetworkError::WebSocket("no se pudieron derivar las llaves de sesión".to_string()))?;
        let mut sealer = Sealer::new(keys.client_to_server);
        let mut opener = Opener::new(keys.server_to_client);

        let auth_request = match (&stored_psk, &password) {
            (Some(psk), _) => AuthRequest::Proof {
                hmac: warlay_crypto::compute_proof(psk, &client_keypair.public_bytes, &welcome.server_ephemeral_pubkey),
            },
            (None, Some(password)) => AuthRequest::Password {
                password: password.clone(),
            },
            (None, None) => AuthRequest::Fresh,
        };
        let sealed_auth = sealer
            .seal(&auth_request.to_json()?)
            .map_err(|_| NetworkError::WebSocket("no se pudo cifrar la solicitud de autenticación".to_string()))?;
        ws_sink
            .send(WsMessage::Binary(sealed_auth))
            .await
            .map_err(|error| NetworkError::WebSocket(error.to_string()))?;

        let auth_frame = ws_stream
            .next()
            .await
            .ok_or_else(|| NetworkError::WebSocket("conexión cerrada durante la autenticación".to_string()))?
            .map_err(|error| NetworkError::WebSocket(error.to_string()))?;
        let WsMessage::Binary(auth_bytes) = auth_frame else {
            return Err(NetworkError::WebSocket(
                "se esperaba una respuesta de autenticación cifrada".to_string(),
            ));
        };
        let auth_plaintext = opener
            .open(&auth_bytes)
            .map_err(|_| NetworkError::WebSocket("no se pudo descifrar la respuesta de autenticación".to_string()))?;
        let new_psk = match AuthResponse::from_json(&auth_plaintext)? {
            AuthResponse::Accepted { psk } => psk,
            AuthResponse::Rejected { message, retry_as_new } => {
                return Err(NetworkError::AuthRejected { message, retry_as_new });
            }
        };

        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<ClientMessage>();

        let writer_task = tokio::spawn(async move {
            while let Some(message) = outbound_rx.recv().await {
                let json = match message.to_json() {
                    Ok(json) => json,
                    Err(error) => {
                        warn!(%error, "failed to encode client message");
                        continue;
                    }
                };
                let sealed = match sealer.seal(json.as_bytes()) {
                    Ok(sealed) => sealed,
                    Err(error) => {
                        warn!(%error, "failed to encrypt client message");
                        continue;
                    }
                };
                if ws_sink.send(WsMessage::Binary(sealed)).await.is_err() {
                    break;
                }
            }
        });

        let last_ping_sent: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        let last_ping_sent_for_reader = last_ping_sent.clone();

        let reader_task = tokio::spawn(async move {
            while let Some(frame) = ws_stream.next().await {
                let frame = match frame {
                    Ok(frame) => frame,
                    Err(error) => {
                        warn!(%error, "websocket read error, closing connection");
                        break;
                    }
                };

                let bytes = match frame {
                    WsMessage::Binary(bytes) => bytes,
                    WsMessage::Close(_) => break,
                    _ => continue,
                };

                let plaintext = match opener.open(&bytes) {
                    Ok(plaintext) => plaintext,
                    Err(error) => {
                        warn!(%error, "failed to decrypt server frame, closing connection");
                        break;
                    }
                };

                let message = match ServerMessage::from_json(&String::from_utf8_lossy(&plaintext)) {
                    Ok(message) => message,
                    Err(error) => {
                        debug!(%error, "ignoring malformed server message");
                        continue;
                    }
                };

                if matches!(message, ServerMessage::Pong) {
                    let sent_at = last_ping_sent_for_reader
                        .lock()
                        .expect("ping timer lock poisoned")
                        .take();
                    if let Some(sent_at) = sent_at {
                        on_ping(sent_at.elapsed().as_millis() as u64);
                    }
                    continue;
                }

                if let ServerMessage::LanPacket { bytes } = message {
                    on_lan_packet(bytes);
                    continue;
                }

                on_message(message);
            }
            on_disconnected();
        });

        let ping_sender = outbound_tx.clone();
        let ping_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(PING_INTERVAL);
            loop {
                interval.tick().await;
                *last_ping_sent.lock().expect("ping timer lock poisoned") = Some(Instant::now());
                if ping_sender.send(ClientMessage::Ping).is_err() {
                    break;
                }
            }
        });

        Ok(ConnectOutcome {
            connection: Self {
                outbound: outbound_tx,
                reader_task,
                writer_task,
                ping_task,
            },
            server_id: welcome.server_id,
            server_name,
            new_psk,
        })
    }

    pub fn send(&self, message: ClientMessage) {
        let _ = self.outbound.send(message);
    }
}

impl Drop for MatchmakingConnection {
    fn drop(&mut self) {
        self.reader_task.abort();
        self.writer_task.abort();
        self.ping_task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    /// Exercises the client's real handshake implementation against the
    /// deployed `warlay-hub` VPS: connect fresh (no stored PSK), get issued
    /// one, authenticate, create a room, then reconnect and prove
    /// possession of the stored PSK instead of re-authenticating fresh.
    /// Not run by default; `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "requires the deployed warlay-hub VPS at 161.132.37.213:7777"]
    async fn full_client_handshake_against_live_vps() {
        let saw_authenticated = Arc::new(AtomicBool::new(false));
        let saw_authenticated_cb = saw_authenticated.clone();

        let outcome = MatchmakingConnection::connect(
            "ws://161.132.37.213:7777/ws",
            Uuid::new_v4(),
            None,
            None,
            move |message| {
                if matches!(message, ServerMessage::Authenticated { .. }) {
                    saw_authenticated_cb.store(true, Ordering::SeqCst);
                }
            },
            |_ping_ms| {},
            |_lan_packet| {},
            || {},
        )
        .await
        .expect("fresh handshake against the live VPS should succeed");

        assert!(outcome.new_psk.is_some(), "a fresh client should be issued a PSK");
        outcome.connection.send(ClientMessage::Authenticate {
            display_name: "PruebaHandshakeCliente".to_string(),
        });

        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            saw_authenticated.load(Ordering::SeqCst),
            "should have received Authenticated"
        );
    }

    /// Same handshake, over `wss://` through the hub's TLS reverse proxy --
    /// proves the certificate verifies against the bundled roots.
    #[tokio::test]
    #[ignore = "requires the deployed warlay-hub at wss://warlay.codesoftlabs.com"]
    async fn full_client_handshake_over_wss_against_live_hub() {
        let outcome = MatchmakingConnection::connect(
            "wss://warlay.codesoftlabs.com/ws",
            Uuid::new_v4(),
            None,
            None,
            |_message| {},
            |_ping_ms| {},
            |_lan_packet| {},
            || {},
        )
        .await
        .expect("fresh handshake over wss should succeed");

        assert!(outcome.new_psk.is_some(), "a fresh client should be issued a PSK");
    }
}
