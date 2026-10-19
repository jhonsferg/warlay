//! Helper (matchmaking) server: UDP rendezvous for the players' direct P2P
//! links and the last-resort UDP relay, both admitting only holders of a
//! room ticket signed by the main server.
//!
//! With an `[helper.edge]` section it is also an *edge*: players keep their
//! realtime socket and chat on it instead of on the main server. Edges are
//! interconnected through the Redis bus (like switches on one network), so
//! a message entering any edge reaches players on every other one, and chat
//! and presence keep working while the main server is down. An edge reads
//! PostgreSQL/Redis directly; it never runs account or social logic.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;
use warlay::core::{UdpRelay, UdpRelayHandle, UdpRendezvous, UdpRendezvousHandle};
use warlay::protocol::HelperHeartbeat;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HelperSettings {
    /// Stable identity; keep it fixed across restarts so rooms stay assigned.
    pub helper_id: Uuid,
    pub name: String,
    pub region: String,
    pub rendezvous_bind: SocketAddr,
    pub relay_bind: SocketAddr,
    /// `host:port` players reach the rendezvous at (the VPS's public address).
    pub public_rendezvous_addr: String,
    /// `host:port` players reach the relay at.
    pub public_relay_addr: String,
    /// Base URL of the main server, e.g. `https://warlay.example.com`.
    /// Unused for a helper embedded in the main server.
    pub main_url: String,
    /// Shared with the main server (its `main.helper_secret`).
    pub helper_secret: String,
    /// Also serve players' realtime socket and chat.
    pub edge: Option<EdgeSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EdgeSettings {
    pub http_bind: SocketAddr,
    /// Base URL players reach this edge at, e.g. `https://helper1.example.com`.
    pub public_url: String,
    pub database_url: String,
    pub redis_url: String,
    pub max_db_connections: u32,
}

impl Default for EdgeSettings {
    fn default() -> Self {
        Self {
            http_bind: "0.0.0.0:8090".parse().expect("valid default addr"),
            public_url: "http://127.0.0.1:8090".into(),
            database_url: "postgres://warlay:warlay@127.0.0.1:5432/warlay".into(),
            redis_url: "redis://127.0.0.1:6379".into(),
            max_db_connections: 5,
        }
    }
}

impl Default for HelperSettings {
    fn default() -> Self {
        Self {
            helper_id: Uuid::nil(),
            name: "helper".into(),
            region: "default".into(),
            rendezvous_bind: "0.0.0.0:7779".parse().expect("valid default addr"),
            relay_bind: "0.0.0.0:7780".parse().expect("valid default addr"),
            public_rendezvous_addr: "127.0.0.1:7779".into(),
            public_relay_addr: "127.0.0.1:7780".into(),
            main_url: "http://127.0.0.1:8080".into(),
            helper_secret: String::new(),
            edge: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HelperConfigFile {
    pub helper: HelperSettings,
    pub observability: crate::main_server::Observability,
}

pub fn load(path: &Path) -> Result<HelperConfigFile> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
}

/// Environment overrides, so one image serves every helper container:
/// `WARLAY_HELPER_SECRET`, `WARLAY_HELPER_NAME`, `WARLAY_HELPER_REGION`,
/// `WARLAY_MAIN_URL`, `WARLAY_PUBLIC_RENDEZVOUS_ADDR`,
/// `WARLAY_PUBLIC_RELAY_ADDR`, and for the edge `WARLAY_EDGE_PUBLIC_URL`
/// (setting it turns the edge on), `WARLAY_DATABASE_URL`, `WARLAY_REDIS_URL`.
pub fn apply_env(config: &mut HelperConfigFile) {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let helper = &mut config.helper;
    if let Some(v) = var("WARLAY_HELPER_SECRET") {
        helper.helper_secret = v;
    }
    if let Some(v) = var("WARLAY_HELPER_NAME") {
        helper.name = v;
    }
    if let Some(v) = var("WARLAY_HELPER_REGION") {
        helper.region = v;
    }
    if let Some(v) = var("WARLAY_MAIN_URL") {
        helper.main_url = v;
    }
    if let Some(v) = var("WARLAY_PUBLIC_RENDEZVOUS_ADDR") {
        helper.public_rendezvous_addr = v;
    }
    if let Some(v) = var("WARLAY_PUBLIC_RELAY_ADDR") {
        helper.public_relay_addr = v;
    }
    if let Some(v) = var("WARLAY_EDGE_PUBLIC_URL") {
        helper.edge.get_or_insert_with(EdgeSettings::default).public_url = v;
    }
    if let Some(edge) = helper.edge.as_mut() {
        if let Some(v) = var("WARLAY_DATABASE_URL") {
            edge.database_url = v;
        }
        if let Some(v) = var("WARLAY_REDIS_URL") {
            edge.redis_url = v;
        }
    }
}

/// A helper's live sockets, shared with whatever reports its load.
pub struct HelperCore {
    pub settings: HelperSettings,
    pub rendezvous: Arc<UdpRendezvous>,
    pub relay: Arc<UdpRelay>,
    /// The edge's shared state, when this helper is one.
    pub edge: Option<warlay::social::AppState>,
}

/// The background tasks serving a helper's sockets.
pub struct HelperTasks {
    pub rendezvous: UdpRendezvousHandle,
    pub relay: UdpRelayHandle,
}

impl HelperCore {
    pub async fn heartbeat(&self) -> HelperHeartbeat {
        HelperHeartbeat {
            helper_id: self.settings.helper_id,
            name: self.settings.name.clone(),
            region: self.settings.region.clone(),
            rendezvous_addr: self.settings.public_rendezvous_addr.clone(),
            relay_addr: self.settings.public_relay_addr.clone(),
            active_rooms: self.rendezvous.active_rooms().await as u32,
            relay_channels: self.relay.active_channels() as u32,
            edge_url: self
                .settings
                .edge
                .as_ref()
                .map(|e| e.public_url.clone())
                .unwrap_or_default(),
            clients: self
                .edge
                .as_ref()
                .map_or(0, |state| state.realtime.local_connections() as u32),
        }
    }
}

pub async fn start(mut settings: HelperSettings) -> Result<(Arc<HelperCore>, HelperTasks)> {
    if settings.helper_secret.len() < 16 {
        anyhow::bail!("helper.helper_secret must match the main server's (at least 16 characters)");
    }
    if settings.helper_id.is_nil() {
        // Derived from the name so a restart keeps the same identity.
        settings.helper_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, settings.name.as_bytes());
    }
    let rendezvous_socket = tokio::net::UdpSocket::bind(settings.rendezvous_bind)
        .await
        .with_context(|| format!("binding rendezvous on {}", settings.rendezvous_bind))?;
    let relay_socket = tokio::net::UdpSocket::bind(settings.relay_bind)
        .await
        .with_context(|| format!("binding relay on {}", settings.relay_bind))?;
    let secret = settings.helper_secret.as_bytes().to_vec();
    let rendezvous = Arc::new(UdpRendezvous::new(rendezvous_socket).with_ticket_secret(secret.clone()));
    let relay = Arc::new(UdpRelay::new(relay_socket, secret));
    info!(
        helper_id = %settings.helper_id,
        name = %settings.name,
        rendezvous = %settings.public_rendezvous_addr,
        relay = %settings.public_relay_addr,
        "helper sockets ready"
    );
    let tasks = HelperTasks {
        rendezvous: rendezvous.clone().spawn(),
        relay: relay.clone().spawn(),
    };
    let edge = match settings.edge.clone() {
        Some(edge) => Some(start_edge(&settings, edge).await?),
        None => None,
    };
    Ok((
        Arc::new(HelperCore {
            settings,
            rendezvous,
            relay,
            edge,
        }),
        tasks,
    ))
}

/// Connects to the shared PostgreSQL/Redis (without migrating: that's the
/// main server's job), joins the chat persister group and serves the edge.
async fn start_edge(settings: &HelperSettings, edge: EdgeSettings) -> Result<warlay::social::AppState> {
    let state = warlay::social::connect(warlay::social::MainSettings {
        database_url: edge.database_url.clone(),
        redis_url: edge.redis_url.clone(),
        helper_secret: settings.helper_secret.clone(),
        run_migrations: false,
        max_db_connections: edge.max_db_connections,
        ..warlay::social::MainSettings::default()
    })
    .await
    .context("connecting the edge to PostgreSQL/Redis")?;
    warlay::social::start_chat_persister(&state, &format!("helper-{}", settings.name));
    let listener = tokio::net::TcpListener::bind(edge.http_bind)
        .await
        .with_context(|| format!("binding the edge on {}", edge.http_bind))?;
    let app = warlay::social::edge_router(state.clone());
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            warn!(%error, "edge server terminated");
        }
    });
    info!(bind = %edge.http_bind, public = %edge.public_url, "edge serving realtime and chat");
    Ok(state)
}

/// Standalone helper: reports to the main server over HTTP every 10s.
pub async fn run(config: HelperConfigFile) -> Result<()> {
    let (helper, tasks) = start(config.helper).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("building HTTP client")?;
    let url = format!(
        "{}/internal/helpers/heartbeat",
        helper.settings.main_url.trim_end_matches('/')
    );

    let beat_helper = helper.clone();
    let heartbeat_task = tokio::spawn(async move {
        let mut reachable = false;
        let mut failures: u32 = 0;
        loop {
            let beat = beat_helper.heartbeat().await;
            // An edge shares Redis with the main server and registers there
            // directly: the registry (and so the edge list players fall back
            // on) stays alive even while the main server is down.
            let result: Result<(), String> = match &beat_helper.edge {
                Some(state) => warlay::social::helpers::record_heartbeat(state, &beat)
                    .await
                    .map_err(|e| e.message),
                None => client
                    .post(&url)
                    .bearer_auth(&beat_helper.settings.helper_secret)
                    .json(&beat)
                    .send()
                    .await
                    .and_then(|r| r.error_for_status())
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
            };
            match result {
                Ok(_) if !reachable => {
                    info!(%url, "registered with the main server");
                    reachable = true;
                    failures = 0;
                }
                Ok(_) => {}
                Err(error) => {
                    // Warn when the link drops, then once a minute while down.
                    if reachable || failures.is_multiple_of(30) {
                        warn!(%error, %url, "heartbeat to the main server failed");
                    }
                    reachable = false;
                    failures += 1;
                }
            }
            // Retry quickly until registered; the main server forgets a
            // helper after 12s without a heartbeat.
            let next = if reachable { 4 } else { 2 };
            tokio::time::sleep(Duration::from_secs(next)).await;
        }
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => info!("shutdown signal received, stopping helper"),
        _ = tasks.rendezvous.task => warn!("rendezvous task ended"),
        _ = tasks.relay.task => warn!("relay task ended"),
    }
    heartbeat_task.abort();
    Ok(())
}
