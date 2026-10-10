use crate::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RelaySettings {
    pub bind_addr: SocketAddr,
    pub control_addr: SocketAddr,
    pub peers: Vec<SocketAddr>,
    pub observation_ttl_secs: u64,
    pub loop_guard_ttl_ms: u64,
}

impl Default for RelaySettings {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:6112".parse().expect("valid default bind addr"),
            control_addr: "0.0.0.0:6116".parse().expect("valid default control addr"),
            peers: Vec::new(),
            observation_ttl_secs: 15,
            loop_guard_ttl_ms: 3_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HubSettings {
    pub ws_addr: SocketAddr,
    pub relay_addr: SocketAddr,
    pub tcp_tunnel_addr: SocketAddr,
    pub rendezvous_udp_addr: SocketAddr,
    pub membership_ttl_secs: u64,
    /// Directory holding the hub's persistent SQLite database (server
    /// identity + issued client PSKs). Point this at a mounted volume in
    /// Docker so it survives container recreation.
    pub data_dir: PathBuf,
    /// How long a client can go without reconnecting before its record
    /// (and issued PSK) is purged, forcing it to authenticate as new.
    pub client_inactivity_purge_days: u64,
    /// How long a room can sit with zero connected participants (host
    /// disconnected, everyone left) before it's purged to free resources.
    /// Deliberately in minutes, not days — unlike client credentials, an
    /// empty room's absence causes no harm to reconnect later, so there's
    /// no reason to let unused ones pile up for long.
    pub room_inactivity_purge_minutes: u64,
}

impl Default for HubSettings {
    fn default() -> Self {
        Self {
            ws_addr: "0.0.0.0:7777".parse().expect("valid default ws addr"),
            relay_addr: "0.0.0.0:6112".parse().expect("valid default relay addr"),
            tcp_tunnel_addr: "0.0.0.0:7778".parse().expect("valid default tcp tunnel addr"),
            rendezvous_udp_addr: "0.0.0.0:7779".parse().expect("valid default rendezvous udp addr"),
            membership_ttl_secs: 30,
            data_dir: PathBuf::from("./data"),
            client_inactivity_purge_days: 30,
            room_inactivity_purge_minutes: 15,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SecuritySettings {
    /// When set, clients must supply this password the first time they
    /// authenticate (over the encrypted handshake channel — see
    /// `lan-bridge-crypto` — never in the clear). Leave unset for an open
    /// server. Not stored anywhere but this config file.
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ObservabilitySettings {
    pub log_level: String,
}

impl Default for ObservabilitySettings {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub server_name: String,
    pub relay: RelaySettings,
    pub hub: HubSettings,
    pub security: SecuritySettings,
    pub observability: ObservabilitySettings,
}

impl Settings {
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path.as_ref()).map_err(|source| ConfigError::Read {
            path: path.as_ref().to_path_buf(),
            source,
        })?;
        toml::from_str(&raw).map_err(ConfigError::Parse)
    }

    pub fn with_defaults(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            relay: RelaySettings::default(),
            hub: HubSettings::default(),
            security: SecuritySettings::default(),
            observability: ObservabilitySettings::default(),
        }
    }
}
