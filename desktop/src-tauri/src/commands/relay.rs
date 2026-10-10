use crate::core::dto::RelayStatusDto;
use crate::core::AppError;
use crate::network::{self, RelayClient};
use crate::state::AppState;
use crate::transport::UdpTransport;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

const LOOP_GUARD_TTL: Duration = Duration::from_secs(3);
const MIN_POLL_INTERVAL_MS: u64 = 500;

/// Starts (or restarts) this machine's own relay tunnel: it will now behave
/// as one more peer in the bridge mesh, reflecting local broadcast traffic
/// to `peers` and re-injecting whatever comes back.
#[tauri::command]
pub async fn start_local_relay(
    app: AppHandle,
    state: State<'_, AppState>,
    bind_addr: String,
    peers: Vec<String>,
) -> Result<(), AppError> {
    let bind: SocketAddr = bind_addr
        .parse()
        .map_err(|_| AppError::InvalidInput(format!("'{bind_addr}' is not a valid bind address")))?;
    let peer_addrs: Vec<SocketAddr> = peers
        .iter()
        .map(|raw| {
            raw.parse()
                .map_err(|_| AppError::InvalidInput(format!("'{raw}' is not a valid peer address")))
        })
        .collect::<Result<_, _>>()?;

    let transport = Arc::new(
        UdpTransport::bind(bind, true)
            .await
            .map_err(network::NetworkError::from)?,
    );
    let client = Arc::new(RelayClient::new(transport, peer_addrs, LOOP_GUARD_TTL));
    let handle = client.spawn();

    {
        let mut guard = state.relay_client.lock().expect("relay client lock poisoned");
        if let Some(previous) = guard.take() {
            previous.abort();
        }
        *guard = Some(handle);
    }

    let _ = app.emit("relay://status-changed", RelayStatusDto { running: true, peers });
    Ok(())
}

/// Stops this machine's relay tunnel, if one is running.
#[tauri::command]
pub async fn stop_local_relay(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    {
        let mut guard = state.relay_client.lock().expect("relay client lock poisoned");
        if let Some(handle) = guard.take() {
            handle.abort();
        }
    }
    let _ = app.emit(
        "relay://status-changed",
        RelayStatusDto {
            running: false,
            peers: Vec::new(),
        },
    );
    Ok(())
}

/// Starts a background poller that periodically queries `relay_ip` for its
/// observed game list and emits `games://updated` events, giving the UI a
/// live-updating list without the frontend needing its own polling timer.
#[tauri::command]
pub async fn watch_games(
    app: AppHandle,
    state: State<'_, AppState>,
    relay_ip: String,
    control_port: u16,
    interval_ms: u64,
) -> Result<(), AppError> {
    let ip: IpAddr = relay_ip
        .parse()
        .map_err(|_| AppError::InvalidInput(format!("'{relay_ip}' is not a valid IP address")))?;
    let target = SocketAddr::new(ip, control_port);
    let poll_interval = Duration::from_millis(interval_ms.max(MIN_POLL_INTERVAL_MS));

    let app_for_task = app.clone();
    let task = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(poll_interval);
        loop {
            ticker.tick().await;
            match network::query_games(target, Duration::from_secs(2)).await {
                Ok(games) => {
                    let dtos: Vec<_> = games
                        .into_iter()
                        .map(|g| crate::core::dto::GameDto {
                            source_addr: g.source_addr.to_string(),
                            game_name: g.game_name,
                            last_seen_secs_ago: g.last_seen_secs_ago,
                            payload_len: g.payload_len,
                        })
                        .collect();
                    let _ = app_for_task.emit("games://updated", &dtos);
                }
                Err(error) => {
                    tracing::warn!(%error, %target, "failed to poll games list");
                }
            }
        }
    });

    let mut guard = state.game_poller.lock().expect("game poller lock poisoned");
    if let Some(previous) = guard.take() {
        previous.abort();
    }
    *guard = Some(task);
    Ok(())
}

/// Stops the background game-list poller, if one is running.
#[tauri::command]
pub async fn stop_watch_games(state: State<'_, AppState>) -> Result<(), AppError> {
    let mut guard = state.game_poller.lock().expect("game poller lock poisoned");
    if let Some(task) = guard.take() {
        task.abort();
    }
    Ok(())
}
