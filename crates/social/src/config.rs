use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// `[main]` section of the main server's configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MainSettings {
    /// Where the REST API and the realtime WebSocket listen.
    pub http_addr: SocketAddr,
    /// e.g. `postgres://warlay:secret@127.0.0.1:5432/warlay`
    pub database_url: String,
    /// e.g. `redis://127.0.0.1:6379`
    pub redis_url: String,
    /// Shared with every helper server: signs room tickets and authenticates
    /// helper heartbeats. Must be long and random in production.
    pub helper_secret: String,
    pub access_token_ttl_secs: u64,
    pub refresh_token_ttl_days: i64,
    /// How long a room ticket stays valid for helpers.
    pub room_ticket_ttl_secs: u64,
    /// Origins allowed to call the API from a browser context (the desktop
    /// app's webview, the dev server). Empty means any origin.
    pub cors_origins: Vec<String>,
    /// Grace period before a disconnected player is removed from its room.
    pub disconnect_grace_secs: u64,
    /// Only the main server migrates the schema; helpers connect to an
    /// already migrated database.
    pub run_migrations: bool,
    /// Connection pool size; helpers run with a small one.
    pub max_db_connections: u32,
}

impl Default for MainSettings {
    fn default() -> Self {
        Self {
            http_addr: "0.0.0.0:8080".parse().expect("valid default http addr"),
            database_url: "postgres://warlay:warlay@127.0.0.1:5432/warlay".into(),
            redis_url: "redis://127.0.0.1:6379".into(),
            helper_secret: String::new(),
            access_token_ttl_secs: 15 * 60,
            refresh_token_ttl_days: 30,
            room_ticket_ttl_secs: 12 * 60 * 60,
            cors_origins: Vec::new(),
            disconnect_grace_secs: 45,
            run_migrations: true,
            max_db_connections: 20,
        }
    }
}
