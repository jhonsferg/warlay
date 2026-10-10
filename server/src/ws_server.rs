use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;
use warlay::crypto::{EphemeralKeypair, Opener, Sealer};
use warlay::matchmaking::{ClientSink, MatchmakingHub};
use warlay::protocol::{AuthRequest, AuthResponse, ClientMessage, HandshakeHello, HandshakeWelcome, ServerMessage};
use warlay::storage::ServerStore;

/// Adapts an `mpsc::UnboundedSender` to the transport-agnostic `ClientSink`
/// trait `MatchmakingHub` depends on, keeping the hub itself unaware that
/// axum or WebSocket exist.
struct MpscSink(mpsc::UnboundedSender<ServerMessage>);

impl ClientSink for MpscSink {
    fn send(&self, message: ServerMessage) {
        let _ = self.0.send(message);
    }
}

pub struct WsServerState {
    pub hub: Arc<MatchmakingHub>,
    pub store: Arc<ServerStore>,
    pub server_name: String,
    pub server_password: Option<String>,
}

pub fn router(state: Arc<WsServerState>) -> Router {
    Router::new().route("/ws", get(ws_handler)).with_state(state)
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<WsServerState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

type WsSender = SplitSink<WebSocket, WsMessage>;
type WsReceiver = SplitStream<WebSocket>;

async fn handle_socket(socket: WebSocket, state: Arc<WsServerState>) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    let Some(hello) = read_handshake_hello(&mut ws_receiver).await else {
        return;
    };
    let client_id = hello.client_id;

    let server_keypair = EphemeralKeypair::generate();
    let welcome = HandshakeWelcome {
        server_id: state.store.server_id(),
        server_name: state.server_name.clone(),
        requires_password: state.server_password.is_some(),
        server_ephemeral_pubkey: server_keypair.public_bytes,
    };
    if !send_plaintext(&mut ws_sender, &welcome).await {
        return;
    }

    let Ok(keys) = server_keypair.derive_session_keys(&hello.client_ephemeral_pubkey) else {
        warn!(%client_id, "failed to derive session keys");
        return;
    };
    let mut sealer = Sealer::new(keys.server_to_client);
    let mut opener = Opener::new(keys.client_to_server);

    let Some(auth_request) = read_encrypted::<AuthRequest>(&mut ws_receiver, &mut opener).await else {
        return;
    };

    let response = authenticate(
        &state,
        client_id,
        &auth_request,
        &hello.client_ephemeral_pubkey,
        &server_keypair.public_bytes,
    );
    let accepted = matches!(response, AuthResponse::Accepted { .. });
    if !send_encrypted(&mut ws_sender, &mut sealer, &response).await || !accepted {
        info!(%client_id, accepted, "handshake did not complete");
        return;
    }

    run_authenticated_session(ws_sender, ws_receiver, sealer, opener, client_id, state).await;
}

fn authenticate(
    state: &WsServerState,
    client_id: Uuid,
    request: &AuthRequest,
    client_pubkey: &[u8; 32],
    server_pubkey: &[u8; 32],
) -> AuthResponse {
    match request {
        AuthRequest::Proof { hmac } => match state.store.get_client(client_id) {
            Ok(Some(record)) if warlay::crypto::verify_proof(&record.psk, client_pubkey, server_pubkey, hmac) => {
                if let Err(error) = state.store.touch_client(client_id) {
                    warn!(%error, %client_id, "failed to refresh client last-seen timestamp");
                }
                AuthResponse::Accepted { psk: None }
            }
            Ok(_) => AuthResponse::Rejected {
                message: "sesión desconocida o expirada, reconectando como nuevo".to_string(),
                retry_as_new: true,
            },
            Err(error) => {
                warn!(%error, %client_id, "storage error while verifying client proof");
                AuthResponse::Rejected {
                    message: "error interno del servidor".to_string(),
                    retry_as_new: false,
                }
            }
        },
        AuthRequest::Password { password } => match &state.server_password {
            Some(expected) if !constant_time_eq::constant_time_eq(expected.as_bytes(), password.as_bytes()) => {
                AuthResponse::Rejected {
                    message: "contraseña incorrecta".to_string(),
                    retry_as_new: false,
                }
            }
            _ => issue_new_psk(state, client_id),
        },
        AuthRequest::Fresh => {
            if state.server_password.is_some() {
                AuthResponse::Rejected {
                    message: "este servidor requiere contraseña".to_string(),
                    retry_as_new: false,
                }
            } else {
                issue_new_psk(state, client_id)
            }
        }
    }
}

fn issue_new_psk(state: &WsServerState, client_id: Uuid) -> AuthResponse {
    let psk = warlay::crypto::generate_psk();
    match state.store.upsert_new_client(client_id, &psk) {
        Ok(()) => AuthResponse::Accepted { psk: Some(psk) },
        Err(error) => {
            warn!(%error, %client_id, "failed to persist newly issued client key");
            AuthResponse::Rejected {
                message: "error interno del servidor".to_string(),
                retry_as_new: false,
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_authenticated_session(
    mut ws_sender: WsSender,
    mut ws_receiver: WsReceiver,
    mut sealer: Sealer,
    mut opener: Opener,
    client_id: Uuid,
    state: Arc<WsServerState>,
) {
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

    let writer_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if !send_encrypted(&mut ws_sender, &mut sealer, &message).await {
                break;
            }
        }
    });

    state.hub.register_client(client_id, Arc::new(MpscSink(tx)));
    info!(%client_id, "client authenticated");

    while let Some(Ok(frame)) = ws_receiver.next().await {
        let WsMessage::Binary(bytes) = frame else {
            if matches!(frame, WsMessage::Close(_)) {
                break;
            }
            continue;
        };
        let Ok(plaintext) = opener.open(&bytes) else {
            debug!(%client_id, "failed to decrypt frame, closing connection");
            break;
        };
        match ClientMessage::from_json(&String::from_utf8_lossy(&plaintext)) {
            Ok(message) => state.hub.handle_message(client_id, message),
            Err(error) => debug!(%error, %client_id, "ignoring malformed client message"),
        }
    }

    state.hub.unregister_client(client_id);
    writer_task.abort();
    info!(%client_id, "client disconnected");
}

async fn read_handshake_hello(ws_receiver: &mut WsReceiver) -> Option<HandshakeHello> {
    let frame = ws_receiver.next().await?.ok()?;
    let WsMessage::Text(text) = frame else {
        debug!("expected a plaintext HandshakeHello, got a different frame type");
        return None;
    };
    match HandshakeHello::from_json(text.as_str()) {
        Ok(hello) => Some(hello),
        Err(error) => {
            debug!(%error, "failed to decode HandshakeHello");
            None
        }
    }
}

async fn read_encrypted<T: serde::de::DeserializeOwned>(
    ws_receiver: &mut WsReceiver,
    opener: &mut Opener,
) -> Option<T> {
    let frame = ws_receiver.next().await?.ok()?;
    let WsMessage::Binary(bytes) = frame else {
        debug!("expected an encrypted binary frame, got a different frame type");
        return None;
    };
    let plaintext = opener.open(&bytes).ok()?;
    serde_json::from_slice(&plaintext).ok()
}

async fn send_plaintext<T: serde::Serialize>(ws_sender: &mut WsSender, value: &T) -> bool {
    let Ok(text) = serde_json::to_string(value) else {
        return false;
    };
    ws_sender.send(WsMessage::Text(text.into())).await.is_ok()
}

async fn send_encrypted<T: serde::Serialize>(ws_sender: &mut WsSender, sealer: &mut Sealer, value: &T) -> bool {
    let Ok(json) = serde_json::to_vec(value) else {
        return false;
    };
    let Ok(framed) = sealer.seal(&json) else {
        return false;
    };
    ws_sender.send(WsMessage::Binary(framed.into())).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::Message as TMessage;
    use warlay::matchmaking::RoomMemberships;

    async fn spawn_test_hub(server_password: Option<&str>) -> (String, tempfile_db::TempDb) {
        let db = tempfile_db::TempDb::new();
        let store = Arc::new(ServerStore::open(&db.path).expect("open test store"));
        let hub = Arc::new(MatchmakingHub::new(Arc::new(RoomMemberships::default())));
        let state = Arc::new(WsServerState {
            hub,
            store,
            server_name: "Servidor de prueba".to_string(),
            server_password: server_password.map(str::to_string),
        });

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = router(state);
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (format!("ws://{addr}/ws"), db)
    }

    /// Performs the client side of the handshake exactly as a real Launcher
    /// client would (using the same `lan-bridge-crypto` primitives), against
    /// a real WS connection. Returns the connection (already past auth, mid
    /// authenticated session) plus the PSK the server issued, if any.
    async fn handshake(
        url: &str,
        client_id: Uuid,
        stored_psk: Option<[u8; 32]>,
        password: Option<&str>,
    ) -> (
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        Sealer,
        Opener,
        AuthResponse,
    ) {
        let (mut ws, _) = tokio_tungstenite::connect_async(url).await.expect("connect");

        let client_keypair = EphemeralKeypair::generate();
        let hello = HandshakeHello {
            client_id,
            client_ephemeral_pubkey: client_keypair.public_bytes,
        };
        ws.send(TMessage::Text(hello.to_json().unwrap())).await.unwrap();

        let welcome_frame = ws.next().await.unwrap().unwrap();
        let TMessage::Text(welcome_text) = welcome_frame else {
            panic!("expected text welcome")
        };
        let welcome = HandshakeWelcome::from_json(welcome_text.as_str()).unwrap();

        let keys = client_keypair
            .derive_session_keys(&welcome.server_ephemeral_pubkey)
            .unwrap();
        let mut sealer = Sealer::new(keys.client_to_server);
        let mut opener = Opener::new(keys.server_to_client);

        let request = match (stored_psk, password) {
            (Some(psk), _) => AuthRequest::Proof {
                hmac: warlay::crypto::compute_proof(
                    &psk,
                    &client_keypair.public_bytes,
                    &welcome.server_ephemeral_pubkey,
                ),
            },
            (None, Some(password)) => AuthRequest::Password {
                password: password.to_string(),
            },
            (None, None) => AuthRequest::Fresh,
        };
        let sealed = sealer.seal(&request.to_json().unwrap()).unwrap();
        ws.send(TMessage::Binary(sealed)).await.unwrap();

        let auth_frame = ws.next().await.unwrap().unwrap();
        let TMessage::Binary(auth_bytes) = auth_frame else {
            panic!("expected binary auth response")
        };
        let plaintext = opener.open(&auth_bytes).unwrap();
        let response = AuthResponse::from_json(&plaintext).unwrap();

        (ws, sealer, opener, response)
    }

    #[tokio::test]
    async fn full_handshake_then_authenticate_on_an_open_server() {
        let (url, _db) = spawn_test_hub(None).await;
        let client_id = Uuid::new_v4();

        let (mut ws, mut sealer, mut opener, response) = handshake(&url, client_id, None, None).await;
        let AuthResponse::Accepted { psk } = response else {
            panic!("expected Accepted")
        };
        assert!(psk.is_some(), "a fresh client should be issued a new PSK");

        let auth_msg = ClientMessage::Authenticate {
            display_name: "Thrall".to_string(),
        };
        let sealed = sealer.seal(auth_msg.to_json().unwrap().as_bytes()).unwrap();
        ws.send(TMessage::Binary(sealed)).await.unwrap();

        let mut saw_authenticated = false;
        for _ in 0..3 {
            let frame = ws.next().await.unwrap().unwrap();
            let TMessage::Binary(bytes) = frame else { continue };
            let plaintext = opener.open(&bytes).unwrap();
            let message = ServerMessage::from_json(&String::from_utf8_lossy(&plaintext)).unwrap();
            if matches!(message, ServerMessage::Authenticated { .. }) {
                saw_authenticated = true;
            }
        }
        assert!(saw_authenticated, "should receive an Authenticated confirmation");
    }

    type TestSocket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

    async fn send_client_message(ws: &mut TestSocket, sealer: &mut Sealer, message: ClientMessage) {
        let sealed = sealer.seal(message.to_json().unwrap().as_bytes()).unwrap();
        ws.send(TMessage::Binary(sealed)).await.unwrap();
    }

    /// Reads frames until `predicate` matches one (returning it) or `tries`
    /// frames have been read without a match (panicking) — every test frame
    /// on this connection is real ciphertext off a real socket, so this
    /// mirrors how a client actually has to wait for a specific broadcast
    /// among whatever else arrives first (e.g. `ServerStats`).
    async fn recv_until(
        ws: &mut TestSocket,
        opener: &mut Opener,
        tries: usize,
        predicate: impl Fn(&ServerMessage) -> bool,
    ) -> ServerMessage {
        for _ in 0..tries {
            let frame = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
                .await
                .expect("timed out waiting for a server message")
                .expect("connection closed")
                .expect("websocket error");
            let TMessage::Binary(bytes) = frame else { continue };
            let plaintext = opener.open(&bytes).unwrap();
            let message = ServerMessage::from_json(&String::from_utf8_lossy(&plaintext)).unwrap();
            if predicate(&message) {
                return message;
            }
        }
        panic!("did not see the expected message within {tries} frames");
    }

    /// Reproduces the exact scenario from a real bug report end to end over
    /// the real wire protocol (not just in-process `hub.rs` calls): two
    /// separate machines ("PC" hosts, "laptop" is already in the room) —
    /// the PC's connection drops, and the room must still be there for the
    /// laptop. Only once the PC reconnects and explicitly deletes it does
    /// the laptop see it go away.
    #[tokio::test]
    async fn room_survives_hosts_disconnect_and_only_the_owner_can_delete_it() {
        let (url, _db) = spawn_test_hub(None).await;
        let pc_client_id = Uuid::new_v4();
        let laptop_client_id = Uuid::new_v4();

        let (mut pc_ws, mut pc_sealer, mut pc_opener, _) = handshake(&url, pc_client_id, None, None).await;
        send_client_message(
            &mut pc_ws,
            &mut pc_sealer,
            ClientMessage::Authenticate {
                display_name: "PC".to_string(),
            },
        )
        .await;
        recv_until(&mut pc_ws, &mut pc_opener, 5, |m| {
            matches!(m, ServerMessage::Authenticated { .. })
        })
        .await;

        let (mut laptop_ws, mut laptop_sealer, mut laptop_opener, _) =
            handshake(&url, laptop_client_id, None, None).await;
        send_client_message(
            &mut laptop_ws,
            &mut laptop_sealer,
            ClientMessage::Authenticate {
                display_name: "Laptop".to_string(),
            },
        )
        .await;
        recv_until(&mut laptop_ws, &mut laptop_opener, 5, |m| {
            matches!(m, ServerMessage::Authenticated { .. })
        })
        .await;

        send_client_message(
            &mut pc_ws,
            &mut pc_sealer,
            ClientMessage::CreateRoom {
                name: "Sala del PC".to_string(),
                game_port: 6112,
                password: None,
            },
        )
        .await;
        let room_id = match recv_until(&mut pc_ws, &mut pc_opener, 5, |m| {
            matches!(m, ServerMessage::RoomCreated { .. })
        })
        .await
        {
            ServerMessage::RoomCreated { room, .. } => room.id,
            _ => unreachable!(),
        };

        // The laptop, already connected before the room existed, must be
        // told about it via the broadcast.
        recv_until(
            &mut laptop_ws,
            &mut laptop_opener,
            5,
            |m| matches!(m, ServerMessage::RoomList { rooms } if rooms.iter().any(|r| r.id == room_id)),
        )
        .await;

        // The PC's connection drops — no LeaveRoom, no DeleteRoom, just gone.
        drop(pc_ws);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // The laptop must NOT see the room removed, only its player count drop.
        let after_disconnect = recv_until(
            &mut laptop_ws,
            &mut laptop_opener,
            5,
            |m| matches!(m, ServerMessage::RoomUpdated { room } if room.id == room_id),
        )
        .await;
        assert!(matches!(after_disconnect, ServerMessage::RoomUpdated { room } if room.player_count == 0));

        // A stranger reconnecting under a different identity can't delete it.
        let stranger_id = Uuid::new_v4();
        let (mut stranger_ws, mut stranger_sealer, mut stranger_opener, _) =
            handshake(&url, stranger_id, None, None).await;
        send_client_message(
            &mut stranger_ws,
            &mut stranger_sealer,
            ClientMessage::Authenticate {
                display_name: "Stranger".to_string(),
            },
        )
        .await;
        recv_until(&mut stranger_ws, &mut stranger_opener, 5, |m| {
            matches!(m, ServerMessage::Authenticated { .. })
        })
        .await;
        send_client_message(
            &mut stranger_ws,
            &mut stranger_sealer,
            ClientMessage::DeleteRoom { room_id },
        )
        .await;
        let rejected = recv_until(&mut stranger_ws, &mut stranger_opener, 5, |m| {
            matches!(m, ServerMessage::Error { .. })
        })
        .await;
        assert!(matches!(rejected, ServerMessage::Error { .. }));

        // The original PC, reconnecting under the SAME persistent
        // client_id, can — ownership survived the disconnect.
        let (mut pc_ws2, mut pc_sealer2, mut pc_opener2, _) = handshake(&url, pc_client_id, None, None).await;
        send_client_message(
            &mut pc_ws2,
            &mut pc_sealer2,
            ClientMessage::Authenticate {
                display_name: "PC".to_string(),
            },
        )
        .await;
        recv_until(&mut pc_ws2, &mut pc_opener2, 5, |m| {
            matches!(m, ServerMessage::Authenticated { .. })
        })
        .await;
        send_client_message(&mut pc_ws2, &mut pc_sealer2, ClientMessage::DeleteRoom { room_id }).await;

        let removed = recv_until(
            &mut laptop_ws,
            &mut laptop_opener,
            5,
            |m| matches!(m, ServerMessage::RoomRemoved { room_id: removed_id } if *removed_id == room_id),
        )
        .await;
        assert!(matches!(removed, ServerMessage::RoomRemoved { .. }));
    }

    #[tokio::test]
    async fn reconnecting_with_the_issued_psk_authenticates_via_proof() {
        let (url, _db) = spawn_test_hub(None).await;
        let client_id = Uuid::new_v4();

        let (_ws, _sealer, _opener, response) = handshake(&url, client_id, None, None).await;
        let AuthResponse::Accepted { psk: Some(psk) } = response else {
            panic!("expected a fresh PSK")
        };

        let (_ws2, _sealer2, _opener2, response2) = handshake(&url, client_id, Some(psk), None).await;
        assert!(matches!(response2, AuthResponse::Accepted { psk: None }));
    }

    #[tokio::test]
    async fn password_protected_server_rejects_wrong_password() {
        let (url, _db) = spawn_test_hub(Some("correcta")).await;
        let client_id = Uuid::new_v4();

        let (_ws, _sealer, _opener, response) = handshake(&url, client_id, None, Some("incorrecta")).await;
        assert!(matches!(
            response,
            AuthResponse::Rejected {
                retry_as_new: false,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn password_protected_server_accepts_correct_password() {
        let (url, _db) = spawn_test_hub(Some("correcta")).await;
        let client_id = Uuid::new_v4();

        let (_ws, _sealer, _opener, response) = handshake(&url, client_id, None, Some("correcta")).await;
        assert!(matches!(response, AuthResponse::Accepted { psk: Some(_) }));
    }

    #[tokio::test]
    async fn unknown_psk_is_rejected_with_retry_as_new() {
        let (url, _db) = spawn_test_hub(None).await;
        let client_id = Uuid::new_v4();

        // A PSK the server never issued (simulates one purged after 30 days).
        let (_ws, _sealer, _opener, response) = handshake(&url, client_id, Some([0_u8; 32]), None).await;
        assert!(matches!(response, AuthResponse::Rejected { retry_as_new: true, .. }));
    }

    mod tempfile_db {
        use std::path::PathBuf;

        pub struct TempDb {
            pub path: PathBuf,
        }

        impl TempDb {
            pub fn new() -> Self {
                let path = std::env::temp_dir().join(format!("warlay-ws-server-test-{}.db", uuid::Uuid::new_v4()));
                Self { path }
            }
        }

        impl Drop for TempDb {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}
