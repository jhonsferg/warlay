use std::sync::Arc;

use redis::aio::ConnectionManager;
use sqlx::PgPool;

use crate::config::MainSettings;
use crate::realtime::Realtime;

/// Everything a request handler needs. Cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    pub settings: Arc<MainSettings>,
    pub realtime: Arc<Realtime>,
}
