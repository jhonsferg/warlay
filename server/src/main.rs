mod cli;
mod logging;
mod ws_server;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, HubArgs, LanMeshArgs, Mode};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};
use warlay::config::Settings;
use warlay::core::{
    ControlResponder, GameObservationStore, PeerRegistry, RelayEngine, RelayHub, TcpTunnelHub, UdpRendezvous,
};
use warlay::matchmaking::{MatchmakingHub, RoomMemberships};
use warlay::storage::ServerStore;
use warlay::transport::UdpTransport;
use ws_server::WsServerState;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.mode {
        Mode::Hub(args) => run_hub(args).await,
        Mode::LanMesh(args) => run_lan_mesh(args).await,
    }
}

/// Centralized matchmaking hub: WebSocket control plane (room lobby) plus a
/// single UDP relay socket shared by every room, demultiplexed by
/// `room_token`. This is the architecture intended for a VPS deployment.
async fn run_hub(args: HubArgs) -> Result<()> {
    let mut settings = match &args.config {
        Some(path) => {
            Settings::load_from_file(path).with_context(|| format!("loading configuration from {}", path.display()))?
        }
        None => Settings::with_defaults("warlay-hub"),
    };
    cli::apply_hub_overrides(&mut settings, &args);

    logging::init(&settings.observability.log_level);

    let store = Arc::new(
        ServerStore::open(&settings.hub.data_dir.join("warlay.db"))
            .context("opening the hub's persistent data store")?,
    );

    info!(
        server_name = %settings.server_name,
        server_id = %store.server_id(),
        ws_addr = %settings.hub.ws_addr,
        relay_addr = %settings.hub.relay_addr,
        tcp_tunnel_addr = %settings.hub.tcp_tunnel_addr,
        rendezvous_udp_addr = %settings.hub.rendezvous_udp_addr,
        requires_password = settings.security.password.is_some(),
        "starting warlay-server (hub mode)"
    );

    let purge_store = store.clone();
    let purge_after_days = settings.hub.client_inactivity_purge_days;
    let cleanup_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let purge_store = purge_store.clone();
            let purged = tokio::task::spawn_blocking(move || {
                purge_store.purge_inactive((purge_after_days * 24 * 60 * 60) as i64)
            })
            .await;
            match purged {
                Ok(Ok(count)) if count > 0 => info!(count, "purged inactive client credentials"),
                Ok(Ok(_)) => {}
                Ok(Err(error)) => warn!(%error, "failed to purge inactive clients"),
                Err(error) => warn!(%error, "client purge task panicked"),
            }
        }
    });

    let relay_transport = Arc::new(
        UdpTransport::bind(settings.hub.relay_addr, false)
            .await
            .context("binding relay hub socket")?,
    );
    let memberships = Arc::new(RoomMemberships::default());
    let relay_hub = Arc::new(RelayHub::new(relay_transport, memberships.clone()));
    let relay_handle = relay_hub.spawn(Duration::from_secs(settings.hub.membership_ttl_secs));

    let tcp_tunnel_listener = tokio::net::TcpListener::bind(settings.hub.tcp_tunnel_addr)
        .await
        .context("binding tcp game-session tunnel listener")?;
    let tcp_tunnel_hub = Arc::new(TcpTunnelHub::new(tcp_tunnel_listener));
    let tcp_tunnel_handle = tcp_tunnel_hub.spawn();

    let rendezvous_socket = tokio::net::UdpSocket::bind(settings.hub.rendezvous_udp_addr)
        .await
        .context("binding udp rendezvous socket")?;
    let rendezvous_hub = Arc::new(UdpRendezvous::new(rendezvous_socket));
    let rendezvous_handle = rendezvous_hub.spawn();

    let matchmaking_hub = Arc::new(MatchmakingHub::new(memberships));

    let room_purge_hub = matchmaking_hub.clone();
    let room_purge_after_secs = settings.hub.room_inactivity_purge_minutes * 60;
    let room_purge_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            let purged = room_purge_hub.purge_inactive_rooms(room_purge_after_secs);
            if purged > 0 {
                info!(count = purged, "purged inactive empty rooms");
            }
        }
    });
    let ws_state = Arc::new(WsServerState {
        hub: matchmaking_hub,
        store: store.clone(),
        server_name: settings.server_name.clone(),
        server_password: settings.security.password.clone(),
    });
    let app = ws_server::router(ws_state);

    let listener = tokio::net::TcpListener::bind(settings.hub.ws_addr)
        .await
        .context("binding matchmaking websocket listener")?;

    let ws_task = tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            error!(%error, "websocket server terminated unexpectedly");
        }
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("shutdown signal received, stopping warlay-server (hub mode)");
        }
        result = relay_handle.data_task => {
            if let Err(join_error) = result {
                error!(%join_error, "relay hub data task terminated unexpectedly");
            }
        }
        result = relay_handle.cleanup_task => {
            if let Err(join_error) = result {
                error!(%join_error, "membership cleanup task terminated unexpectedly");
            }
        }
        result = tcp_tunnel_handle.accept_task => {
            if let Err(join_error) = result {
                error!(%join_error, "tcp tunnel accept task terminated unexpectedly");
            }
        }
        result = rendezvous_handle.task => {
            if let Err(join_error) = result {
                error!(%join_error, "udp rendezvous task terminated unexpectedly");
            }
        }
        result = cleanup_task => {
            if let Err(join_error) = result {
                error!(%join_error, "client credential cleanup task terminated unexpectedly");
            }
        }
        result = room_purge_task => {
            if let Err(join_error) = result {
                error!(%join_error, "room purge task terminated unexpectedly");
            }
        }
        result = ws_task => {
            if let Err(join_error) = result {
                error!(%join_error, "websocket server task terminated unexpectedly");
            }
        }
    }

    Ok(())
}

/// LAN-mesh mode: broadcast-reflection peer mesh between statically
/// configured bridge instances on isolated subnets (e.g. Termux on a phone
/// hotspot). Unchanged from the original architecture.
async fn run_lan_mesh(args: LanMeshArgs) -> Result<()> {
    let mut settings = match &args.config {
        Some(path) => {
            Settings::load_from_file(path).with_context(|| format!("loading configuration from {}", path.display()))?
        }
        None => Settings::with_defaults("warlay-server"),
    };
    cli::apply_lan_mesh_overrides(&mut settings, &args);

    logging::init(&settings.observability.log_level);

    info!(
        server_name = %settings.server_name,
        relay_addr = %settings.relay.bind_addr,
        control_addr = %settings.relay.control_addr,
        peer_count = settings.relay.peers.len(),
        "starting warlay-server (lan-mesh mode)"
    );

    let data_transport = Arc::new(
        UdpTransport::bind(settings.relay.bind_addr, true)
            .await
            .context("binding relay data socket")?,
    );
    let control_transport = Arc::new(
        UdpTransport::bind(settings.relay.control_addr, false)
            .await
            .context("binding relay control socket")?,
    );

    let peers = Arc::new(PeerRegistry::new(settings.relay.peers.clone()));
    let observations = Arc::new(GameObservationStore::default());

    let relay_engine = Arc::new(RelayEngine::new(
        data_transport,
        peers.clone(),
        observations.clone(),
        Duration::from_millis(settings.relay.loop_guard_ttl_ms),
    ));
    let relay_handle = relay_engine.spawn(Duration::from_secs(settings.relay.observation_ttl_secs));

    let control_responder = Arc::new(ControlResponder::new(
        control_transport,
        peers,
        observations,
        settings.server_name.clone(),
        settings.relay.bind_addr.port(),
    ));
    let control_task = control_responder.spawn();

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("shutdown signal received, stopping warlay-server (lan-mesh mode)");
        }
        result = relay_handle.data_task => {
            if let Err(join_error) = result {
                error!(%join_error, "relay data task terminated unexpectedly");
            }
        }
        result = relay_handle.cleanup_task => {
            if let Err(join_error) = result {
                error!(%join_error, "observation cleanup task terminated unexpectedly");
            }
        }
        result = control_task => {
            if let Err(join_error) = result {
                error!(%join_error, "control task terminated unexpectedly");
            }
        }
    }

    Ok(())
}
