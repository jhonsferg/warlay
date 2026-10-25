use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use warlay::protocol::ipc::{DaemonRequestEnvelope, DaemonResponse, DaemonResponseEnvelope};

use crate::handlers::handle_request;
use crate::state::DaemonState;

/// Wraps a server-initiated broadcast (not a direct reply to any request) in
/// an envelope with no correlation id.
pub fn event(response: DaemonResponse) -> DaemonResponseEnvelope {
    DaemonResponseEnvelope {
        reply_to: None,
        response,
    }
}

/// Shorthand for a `DaemonResponse::Error` reply.
pub fn err(request_type: &str, message: impl std::fmt::Display) -> DaemonResponse {
    DaemonResponse::Error {
        request_type: request_type.to_string(),
        message: message.to_string(),
    }
}

/// Pushes a state-broadcast event to whichever pipe connection is currently
/// live (`DaemonState::current_tx`), looked up fresh at send time rather
/// than captured once at connect time -- so a UI restart doesn't leave
/// long-lived callbacks (hub messages, pings, tunnel status) writing into a
/// dead channel forever.
pub async fn push_event(state: &Arc<Mutex<DaemonState>>, response: DaemonResponse) {
    let tx = state.lock().await.current_tx.clone();
    match tx {
        Some(tx) => {
            let _ = tx.try_send(event(response));
        }
        None => debug!("evento descartado: no hay una UI conectada al pipe en este momento"),
    }
}

/// Runs the full request/response lifecycle for one already-accepted,
/// already-identity-verified client connection (a named pipe on Windows, a
/// Unix socket on Linux): registers it as the current "UI" target for push
/// events (resyncing it with whatever's already live, so a UI restart
/// doesn't go dark until the next state change), then reads
/// newline-delimited JSON requests until the client disconnects or the
/// connection errors.
pub async fn handle_connection<S>(connected_client: S, state_clone: Arc<Mutex<DaemonState>>)
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (reader, mut writer) = tokio::io::split(connected_client);
    let mut reader = BufReader::new(reader);

    let (tx, mut rx) = tokio::sync::mpsc::channel::<DaemonResponseEnvelope>(32);

    {
        let mut s = state_clone.lock().await;
        s.current_tx = Some(tx.clone());
        for server_id in s.connections.keys() {
            let _ = tx.try_send(event(DaemonResponse::HubConnectionState {
                server_id: server_id.clone(),
                connected: true,
            }));
        }
        let _ = tx.try_send(event(DaemonResponse::TunnelStatusChanged {
            running: s.game_tunnel.is_some(),
        }));
    }

    // Writer Task
    let writer_task = tokio::spawn(async move {
        while let Some(resp) = rx.recv().await {
            let mut response_str = serde_json::to_string(&resp).unwrap();
            response_str.push('\n');
            if let Err(e) = writer.write_all(response_str.as_bytes()).await {
                warn!(error = %e, "writer task error");
                break;
            }
        }
    });

    // Reader Task
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                info!("client disconnected");
                break;
            }
            Ok(_) => {
                let msg = line.trim();
                if msg.is_empty() {
                    continue;
                }

                // Deliberately never logs the raw request line: it
                // can carry ConnectToHub's psk/password in plaintext.
                match serde_json::from_str::<DaemonRequestEnvelope>(msg) {
                    Ok(envelope) => {
                        // At `info` (not `debug`) deliberately: this is the
                        // one log line guaranteed to fire for every single
                        // request under the daemon's default filter, so a
                        // request never vanishes with zero trace even if its
                        // specific handler branch has nothing more to say.
                        info!(
                            request = envelope.request.variant_name(),
                            id = envelope.id,
                            "handling request"
                        );
                        let response = handle_request(envelope.request, state_clone.clone()).await;
                        let _ = tx
                            .send(DaemonResponseEnvelope {
                                reply_to: Some(envelope.id),
                                response,
                            })
                            .await;
                    }
                    Err(e) => {
                        warn!(error = %e, "invalid JSON request");
                        let response = err("Unknown", format!("parse error: {e}"));
                        let _ = tx.send(event(response)).await;
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "pipe read error");
                break;
            }
        }
    }
    writer_task.abort();

    // Only clear current_tx if nobody reconnected in the
    // meantime and claimed it already.
    let mut s = state_clone.lock().await;
    let still_ours = matches!(&s.current_tx, Some(t) if t.same_channel(&tx));
    if still_ours {
        s.current_tx = None;
    }
}
