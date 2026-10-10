use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{Emitter, Manager};

use crate::state::AppState;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::ClientOptions;
use tokio::sync::{mpsc, oneshot};
use warlay::protocol::ipc::{DaemonRequest, DaemonRequestEnvelope, DaemonResponse, DaemonResponseEnvelope};

#[derive(Debug, Error)]
pub enum DaemonIpcError {
    #[error("failed to reach the daemon: {0}")]
    Send(String),
    #[error("el daemon no respondió a tiempo")]
    Timeout,
    #[error("la conexión con el daemon se cerró antes de responder")]
    Closed,
}

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<DaemonResponse>>>>;

#[derive(Clone)]
pub struct DaemonClient {
    tx: mpsc::Sender<DaemonRequestEnvelope>,
    pending: PendingMap,
    next_id: Arc<AtomicU64>,
}

impl DaemonClient {
    pub async fn connect(pipe_name: &str, app_handle: tauri::AppHandle) -> std::io::Result<Self> {
        let client = ClientOptions::new().open(pipe_name)?;

        let (reader, mut writer) = tokio::io::split(client);
        let mut reader = BufReader::new(reader);

        let (tx, mut rx) = mpsc::channel::<DaemonRequestEnvelope>(32);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let pending_for_reader = pending.clone();

        // Writer Task (Client -> Daemon)
        tokio::spawn(async move {
            while let Some(envelope) = rx.recv().await {
                if let Ok(json) = serde_json::to_string(&envelope) {
                    let _ = writer.write_all(format!("{}\n", json).as_bytes()).await;
                }
            }
        });

        // Reader Task (Daemon -> Client)
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // Disconnected
                    Ok(_) => {
                        let msg = line.trim();
                        if msg.is_empty() {
                            continue;
                        }

                        let Ok(envelope) = serde_json::from_str::<DaemonResponseEnvelope>(msg) else {
                            continue;
                        };

                        // A direct reply to an in-flight `call()` -- resolve
                        // its oneshot instead of routing it as an event.
                        if let Some(id) = envelope.reply_to {
                            let waiter = pending_for_reader.lock().unwrap().remove(&id);
                            if let Some(waiter) = waiter {
                                let _ = waiter.send(envelope.response);
                                continue;
                            }
                        }

                        match envelope.response {
                            DaemonResponse::HubConnectionState { server_id, connected } => {
                                use crate::core::dto::ServerStatusEvent;
                                let _ = app_handle
                                    .emit("matchmaking://status", &ServerStatusEvent { server_id, connected });
                            }
                            DaemonResponse::HubPing { server_id, latency_ms } => {
                                use crate::core::dto::ServerPingEvent;
                                let _ = app_handle.emit(
                                    "matchmaking://ping",
                                    &ServerPingEvent {
                                        server_id,
                                        ping_ms: latency_ms as u64,
                                    },
                                );
                            }
                            DaemonResponse::HubMessage { server_id, message } => {
                                use crate::core::dto::ServerMessageEvent;
                                use warlay::protocol::ServerMessage;

                                // Kept current here (not just relayed) so a
                                // freshly-created window with no history of
                                // *past* pushes -- the overlay, opened only
                                // once the game launches, well after the
                                // room may already have members -- has
                                // somewhere to ask for the truth on mount
                                // instead of only ever learning about
                                // members from a *new* push. See
                                // `AppState::active_room`'s doc comment.
                                let active_room = &app_handle.state::<AppState>().active_room;
                                match &message {
                                    ServerMessage::RoomCreated { room, .. }
                                    | ServerMessage::JoinedRoom { room, .. } => {
                                        *active_room.lock().unwrap() = Some((room.id, Vec::new()));
                                    }
                                    ServerMessage::RoomMembersChanged { room_id, members } => {
                                        let mut guard = active_room.lock().unwrap();
                                        if let Some((active_id, active_members)) = guard.as_mut() {
                                            if active_id == room_id {
                                                *active_members = members.clone();
                                            }
                                        }
                                    }
                                    ServerMessage::LeftRoom => {
                                        *active_room.lock().unwrap() = None;
                                    }
                                    ServerMessage::RoomRemoved { room_id } => {
                                        let mut guard = active_room.lock().unwrap();
                                        if guard.as_ref().is_some_and(|(active_id, _)| active_id == room_id) {
                                            *guard = None;
                                        }
                                    }
                                    _ => {}
                                }

                                let _ = app_handle
                                    .emit("matchmaking://message", &ServerMessageEvent { server_id, message });
                            }
                            DaemonResponse::GameStarted => {
                                let _ = app_handle.emit("session://game-started", ());
                            }
                            DaemonResponse::GameEnded => {
                                let _ = app_handle.emit("session://game-ended", ());
                            }
                            DaemonResponse::TunnelStatusChanged { running } => {
                                let _ = app_handle.emit("hubTunnel://status-changed", running);
                            }
                            DaemonResponse::VoiceFrameIn {
                                from_client_id,
                                opus_bytes,
                            } => {
                                use crate::core::dto::VoiceFrameEvent;
                                let _ = app_handle.emit(
                                    "voice://frame-in",
                                    &VoiceFrameEvent {
                                        from_client_id,
                                        opus_bytes,
                                    },
                                );
                            }
                            _ => {
                                // Success/Error/Pong/Connected/ConnectFailed with no
                                // (or a since-timed-out) matching pending call --
                                // nothing left to route it to.
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error reading from daemon: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(Self {
            tx,
            pending,
            next_id: Arc::new(AtomicU64::new(1)),
        })
    }

    /// Sends a request and awaits the daemon's direct reply, correlated by
    /// id. Times out after 10s so a dead/hung daemon fails the calling
    /// Tauri command instead of hanging it forever.
    pub async fn call(&self, req: DaemonRequest) -> Result<DaemonResponse, DaemonIpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (resp_tx, resp_rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, resp_tx);

        if self.tx.send(DaemonRequestEnvelope { id, request: req }).await.is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Err(DaemonIpcError::Send("daemon channel closed".to_string()));
        }

        match tokio::time::timeout(Duration::from_secs(10), resp_rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(DaemonIpcError::Closed),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(DaemonIpcError::Timeout)
            }
        }
    }
}
