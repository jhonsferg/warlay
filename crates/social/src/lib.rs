//! Warlay's main server: accounts, sessions, friends, communities, rooms,
//! chat, invitations, notifications and presence, over a REST API plus a
//! realtime WebSocket. Backed by PostgreSQL (durable data) and Redis
//! (access tokens, presence, rate limits, helper registry and the pub/sub
//! bus that fans realtime events out across instances).
//!
//! It never carries game traffic: rooms are assigned to helper servers
//! (`warlay-server helper`), which do rendezvous for the players' direct
//! P2P links and, only as a last resort, relay them.

mod auth;
pub mod chat;
pub mod config;
mod error;
pub mod helpers;
mod models;
pub mod realtime;
mod receipts;
mod routes;
mod schema;
mod state;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::http::{HeaderValue, Method};
use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tracing::info;

pub use config::MainSettings;
pub use error::ApiError;
pub use state::AppState;

use routes::{auth as auth_routes, chat as chat_routes, communities, friends, invites, notifications, rooms, users};

/// Connects to Postgres (running migrations) and Redis, and starts the
/// realtime subscriber.
pub async fn connect(settings: MainSettings) -> anyhow::Result<AppState> {
    if settings.helper_secret.len() < 16 {
        anyhow::bail!("main.helper_secret must be set to a random string of at least 16 characters");
    }
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(settings.max_db_connections)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&settings.database_url)
        .await
        .context("connecting to PostgreSQL")?;
    if settings.run_migrations {
        schema::migrate(&db).await.context("running database migrations")?;
    }
    let redis_client = redis::Client::open(settings.redis_url.as_str()).context("parsing redis_url")?;
    let redis = redis::aio::ConnectionManager::new(redis_client.clone())
        .await
        .context("connecting to Redis")?;
    let realtime = realtime::Realtime::new(redis.clone());
    realtime.start_subscriber(redis_client);
    info!("connected to PostgreSQL and Redis");
    Ok(AppState {
        db,
        redis,
        settings: Arc::new(settings),
        realtime,
    })
}

fn cors(settings: &MainSettings) -> CorsLayer {
    let layer = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any);
    if settings.cors_origins.is_empty() {
        layer.allow_origin(Any)
    } else {
        let origins: Vec<HeaderValue> = settings
            .cors_origins
            .iter()
            .filter_map(|o| HeaderValue::from_str(o).ok())
            .collect();
        layer.allow_origin(AllowOrigin::list(origins))
    }
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/auth/register", post(auth_routes::register))
        .route("/auth/login", post(auth_routes::login))
        .route("/auth/refresh", post(auth_routes::refresh))
        .route("/auth/logout", post(auth_routes::logout))
        .route("/me", get(auth_routes::me).patch(auth_routes::update_me))
        .route("/me/password", post(auth_routes::change_password))
        .route("/users/search", get(users::search))
        .route("/users/{id}", get(users::profile))
        .route("/friends", get(friends::list))
        .route("/friends/{id}", delete(friends::remove))
        .route("/friends/requests", get(friends::requests).post(friends::send_request))
        .route("/friends/requests/{id}/accept", post(friends::accept))
        .route("/friends/requests/{id}", delete(friends::decline))
        .route("/communities", get(communities::mine).post(communities::create))
        .route("/communities/discover", get(communities::discover))
        .route(
            "/communities/{id}",
            get(communities::detail).delete(communities::delete),
        )
        .route("/communities/{id}/join", post(communities::join))
        .route("/communities/{id}/leave", post(communities::leave))
        .route(
            "/communities/{id}/members/{user_id}",
            delete(communities::remove_member),
        )
        .route("/communities/{id}/invites", post(invites::invite_to_community))
        .route("/rooms", get(rooms::list).post(rooms::create))
        .route("/rooms/current", get(rooms::current))
        .route("/rooms/{id}", get(rooms::detail).delete(rooms::delete))
        .route("/rooms/{id}/join", post(rooms::join))
        .route("/rooms/{id}/leave", post(rooms::leave))
        .route("/rooms/{id}/session", get(rooms::session))
        .route("/rooms/{id}/status", post(rooms::set_status))
        .route("/rooms/{id}/members/{user_id}", delete(rooms::kick))
        .route("/rooms/{id}/invites", post(invites::invite_to_room))
        .route("/invites", get(invites::pending))
        .route("/invites/{id}/accept", post(invites::accept))
        .route("/invites/{id}/decline", post(invites::decline))
        .route(
            "/chat/{kind}/{id}/messages",
            get(chat_routes::history).post(chat_routes::send),
        )
        .route("/chat/{kind}/{id}/messages/{message_id}", delete(chat_routes::delete_message))
        .route(
            "/chat/{kind}/{id}/receipt",
            get(chat_routes::peer_receipt).post(chat_routes::mark_receipt),
        )
        .route("/chat/unread", get(chat_routes::unread))
        .route("/notifications", get(notifications::list))
        .route("/notifications/read-all", post(notifications::mark_all_read))
        .route("/notifications/{id}/read", post(notifications::mark_read))
        .route("/ws", get(realtime::ws_handler))
        .route("/edges", get(helpers::edges_handler));

    Router::new()
        .nest("/api/v1", api)
        .route("/internal/helpers/heartbeat", post(helpers::heartbeat_handler))
        .route("/health", get(|| async { "ok" }))
        .layer(cors(&state.settings))
        .with_state(state)
}

/// What a helper serves to players as their edge: the realtime socket and
/// chat. Everything else (accounts, friends, rooms...) stays on the main
/// server.
pub fn edge_router(state: AppState) -> Router {
    let api = Router::new()
        .route(
            "/chat/{kind}/{id}/messages",
            get(chat_routes::history).post(chat_routes::send),
        )
        .route("/chat/{kind}/{id}/messages/{message_id}", delete(chat_routes::delete_message))
        .route(
            "/chat/{kind}/{id}/receipt",
            get(chat_routes::peer_receipt).post(chat_routes::mark_receipt),
        )
        .route("/chat/unread", get(chat_routes::unread))
        .route("/ws", get(realtime::ws_handler));
    Router::new()
        .nest("/api/v1", api)
        .route("/health", get(|| async { "ok" }))
        .layer(cors(&state.settings))
        .with_state(state)
}

/// Closes the rooms whose host has been away longer than
/// `host_absence_close_secs` (the watchdog does this every minute). Exposed
/// for maintenance tooling and tests.
pub async fn reap_abandoned_rooms(state: &AppState) {
    if let Err(error) = routes::rooms::reap_abandoned_rooms(state).await {
        tracing::warn!(?error, "abandoned room reaper failed");
    }
}

/// Starts this node's share of persisting chat to PostgreSQL.
/// Main server only: moves rooms off helpers that died.
pub fn start_helper_watchdog(state: &AppState) {
    routes::rooms::spawn_helper_watchdog(state.clone());
}

pub fn start_chat_persister(state: &AppState, node: &str) {
    crate::chat::spawn_persister(state.clone(), node.to_string());
}
