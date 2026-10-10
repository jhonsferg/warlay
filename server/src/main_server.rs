//! Main server: the social backend (accounts, friends, communities, rooms,
//! chat, notifications). Optionally runs a helper in-process, so a single
//! VPS can host the whole service.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use warlay::social::MainSettings;

use crate::helper::{self, HelperSettings};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Observability {
    pub log_level: String,
}

impl Default for Observability {
    fn default() -> Self {
        Self {
            log_level: "info".into(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MainConfigFile {
    pub main: MainSettings,
    /// When present, a helper runs inside this process and registers itself
    /// directly (its `helper_secret`/`main_url` are taken from `[main]`).
    pub embedded_helper: Option<HelperSettings>,
    pub observability: Observability,
}

pub fn load(path: &Path) -> Result<MainConfigFile> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
}

/// Environment overrides, handy for containers: `WARLAY_DATABASE_URL`,
/// `WARLAY_REDIS_URL`, `WARLAY_HELPER_SECRET`.
pub fn apply_env(config: &mut MainConfigFile) {
    if let Ok(v) = std::env::var("WARLAY_DATABASE_URL") {
        config.main.database_url = v;
    }
    if let Ok(v) = std::env::var("WARLAY_REDIS_URL") {
        config.main.redis_url = v;
    }
    if let Ok(v) = std::env::var("WARLAY_HELPER_SECRET") {
        config.main.helper_secret = v;
    }
}

pub async fn run(config: MainConfigFile) -> Result<()> {
    let state = warlay::social::connect(config.main.clone()).await?;
    warlay::social::start_chat_persister(&state, "main");
    warlay::social::start_helper_watchdog(&state);

    let embedded_task = match config.embedded_helper.clone() {
        Some(mut settings) => {
            settings.helper_secret = config.main.helper_secret.clone();
            let (core, tasks) = helper::start(settings).await?;
            let beat_state = state.clone();
            Some(tokio::spawn(async move {
                let _tasks = tasks;
                let mut interval = tokio::time::interval(Duration::from_secs(4));
                loop {
                    interval.tick().await;
                    let beat = core.heartbeat().await;
                    if let Err(error) = warlay::social::helpers::record_heartbeat(&beat_state, &beat).await {
                        warn!(?error, "embedded helper failed to register");
                    }
                }
            }))
        }
        None => None,
    };

    let app = warlay::social::router(state);
    let listener = tokio::net::TcpListener::bind(config.main.http_addr)
        .await
        .with_context(|| format!("binding {}", config.main.http_addr))?;
    info!(addr = %config.main.http_addr, embedded_helper = embedded_task.is_some(), "main server listening");

    tokio::select! {
        result = axum::serve(listener, app) => {
            if let Err(error) = result {
                error!(%error, "HTTP server terminated");
            }
        }
        _ = tokio::signal::ctrl_c() => info!("shutdown signal received, stopping main server"),
    }
    if let Some(task) = embedded_task {
        task.abort();
    }
    Ok(())
}
