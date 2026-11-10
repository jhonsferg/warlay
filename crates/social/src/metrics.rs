//! Metrics in the Prometheus text format, at `GET /metrics`.
//!
//! Off until `metrics_token` is set; then every scrape must send it as
//! `Authorization: Bearer <token>`. Request counters and latencies are kept in
//! memory by a middleware (by route template, so the labels stay few); the
//! numbers that come from the database are read at scrape time and kept for ten
//! seconds, so a busy scraper does not become load on PostgreSQL.

use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{MatchedPath, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

/// Upper bounds, in seconds, of the latency histogram.
const BUCKETS: [f64; 11] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0];
/// A series per (method, route, status) is bounded by the routes the server has;
/// this is a backstop, not a limit anyone should reach.
const MAX_SERIES: usize = 4000;
const GAUGE_CACHE: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Histogram {
    /// Observations at or below each bound (not cumulative).
    counts: [u64; BUCKETS.len() + 1],
    sum_secs: f64,
    count: u64,
}

#[derive(Default, Clone)]
struct Gauges {
    users: i64,
    sessions_active: i64,
    communities: i64,
    reports_open: i64,
    users_with_totp: i64,
    google_identities: i64,
    rooms_by_status: Vec<(String, i64)>,
    helpers: Vec<HelperGauge>,
}

#[derive(Clone)]
struct HelperGauge {
    name: String,
    region: String,
    active_rooms: u32,
    relay_channels: u32,
    clients: u32,
}

pub struct Metrics {
    started: Instant,
    requests: Mutex<HashMap<(String, String, u16), u64>>,
    latency: Mutex<HashMap<(String, String), Histogram>>,
    gauges: Mutex<Option<(Instant, Gauges)>>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            requests: Mutex::new(HashMap::new()),
            latency: Mutex::new(HashMap::new()),
            gauges: Mutex::new(None),
        }
    }

    pub fn observe(&self, method: &str, route: &str, status: u16, took: Duration) {
        {
            let mut requests = self.requests.lock().expect("metrics lock");
            let key = (method.to_string(), route.to_string(), status);
            if requests.len() < MAX_SERIES || requests.contains_key(&key) {
                *requests.entry(key).or_default() += 1;
            }
        }
        let mut latency = self.latency.lock().expect("metrics lock");
        let key = (method.to_string(), route.to_string());
        if latency.len() < MAX_SERIES || latency.contains_key(&key) {
            let histogram = latency.entry(key).or_default();
            let secs = took.as_secs_f64();
            let slot = BUCKETS.iter().position(|bound| secs <= *bound).unwrap_or(BUCKETS.len());
            histogram.counts[slot] += 1;
            histogram.sum_secs += secs;
            histogram.count += 1;
        }
    }

    fn render_requests(&self, out: &mut String) {
        let requests = self.requests.lock().expect("metrics lock");
        let mut rows: Vec<_> = requests.iter().collect();
        rows.sort();
        header_lines(
            out,
            "warlay_http_requests_total",
            "counter",
            "HTTP requests served, by route template and status.",
        );
        for ((method, route, status), count) in rows {
            let _ = writeln!(
                out,
                "warlay_http_requests_total{{method=\"{}\",route=\"{}\",status=\"{status}\"}} {count}",
                escape(method),
                escape(route)
            );
        }
        drop(requests);

        let latency = self.latency.lock().expect("metrics lock");
        let mut rows: Vec<_> = latency.iter().collect();
        rows.sort_by(|a, b| a.0.cmp(b.0));
        header_lines(
            out,
            "warlay_http_request_duration_seconds",
            "histogram",
            "Time to answer an HTTP request, by route template.",
        );
        for ((method, route), histogram) in rows {
            let (method, route) = (escape(method), escape(route));
            let mut cumulative = 0;
            for (i, bound) in BUCKETS.iter().enumerate() {
                cumulative += histogram.counts[i];
                let _ = writeln!(
                    out,
                    "warlay_http_request_duration_seconds_bucket{{method=\"{method}\",route=\"{route}\",le=\"{bound}\"}} {cumulative}"
                );
            }
            let _ = writeln!(
                out,
                "warlay_http_request_duration_seconds_bucket{{method=\"{method}\",route=\"{route}\",le=\"+Inf\"}} {}",
                histogram.count
            );
            let _ = writeln!(
                out,
                "warlay_http_request_duration_seconds_sum{{method=\"{method}\",route=\"{route}\"}} {}",
                histogram.sum_secs
            );
            let _ = writeln!(
                out,
                "warlay_http_request_duration_seconds_count{{method=\"{method}\",route=\"{route}\"}} {}",
                histogram.count
            );
        }
    }
}

fn header_lines(out: &mut String, name: &str, kind: &str, help: &str) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} {kind}");
}

fn gauge(out: &mut String, name: &str, help: &str, value: impl std::fmt::Display) {
    header_lines(out, name, "gauge", help);
    let _ = writeln!(out, "{name} {value}");
}

/// Label values escape backslash, double quote and newline.
fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

/// Counts every request that reaches a route, by the route's template (so
/// `/api/v1/rooms/{id}` is one series, not one per room).
pub async fn track(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = request.method().as_str().to_string();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "unmatched".to_string());
    let response = next.run(request).await;
    state
        .metrics
        .observe(&method, &route, response.status().as_u16(), started.elapsed());
    response
}

async fn load_gauges(state: &AppState) -> Gauges {
    let count = |sql: &'static str| async move {
        sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(&state.db)
            .await
            .unwrap_or(-1)
    };
    let rooms_by_status: Vec<(String, i64)> = sqlx::query_as("SELECT status, count(*) FROM rooms GROUP BY status")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    let helpers = crate::helpers::live_helpers(state)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|h| HelperGauge {
            name: h.name,
            region: h.region,
            active_rooms: h.active_rooms,
            relay_channels: h.relay_channels,
            clients: h.clients,
        })
        .collect();
    Gauges {
        users: count("SELECT count(*) FROM users").await,
        sessions_active: count("SELECT count(*) FROM sessions WHERE revoked_at IS NULL AND expires_at > now()").await,
        communities: count("SELECT count(*) FROM communities").await,
        reports_open: count("SELECT count(*) FROM user_reports WHERE status = 'open'").await,
        users_with_totp: count("SELECT count(*) FROM user_totp WHERE enabled_at IS NOT NULL").await,
        google_identities: count("SELECT count(*) FROM external_identities WHERE provider = 'google'").await,
        rooms_by_status,
        helpers,
    }
}

async fn gauges(state: &AppState) -> Gauges {
    {
        let cached = state.metrics.gauges.lock().expect("metrics lock");
        if let Some((at, values)) = cached.as_ref() {
            if at.elapsed() < GAUGE_CACHE {
                return values.clone();
            }
        }
    }
    let fresh = load_gauges(state).await;
    *state.metrics.gauges.lock().expect("metrics lock") = Some((Instant::now(), fresh.clone()));
    fresh
}

fn authorized(state: &AppState, headers: &HeaderMap) -> bool {
    let expected = &state.settings.metrics_token;
    if expected.is_empty() {
        return false;
    }
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    constant_time_eq::constant_time_eq(given.as_bytes(), expected.as_bytes())
}

/// `GET /metrics`.
pub async fn scrape(State(state): State<AppState>, headers: HeaderMap) -> Response {
    // Off (no token set) looks like there is nothing here; a wrong token is refused.
    if state.settings.metrics_token.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, "Bearer")]).into_response();
    }
    let mut out = String::new();
    header_lines(&mut out, "warlay_build_info", "gauge", "Version of the running server.");
    let _ = writeln!(out, "warlay_build_info{{version=\"{}\"}} 1", env!("CARGO_PKG_VERSION"));
    gauge(
        &mut out,
        "warlay_uptime_seconds",
        "Seconds since the server started.",
        state.metrics.started.elapsed().as_secs(),
    );
    gauge(
        &mut out,
        "warlay_realtime_connections",
        "Realtime sockets open on this instance.",
        state.realtime.connection_count(),
    );
    gauge(
        &mut out,
        "warlay_db_pool_connections",
        "Connections in the PostgreSQL pool, in use and idle.",
        state.db.size(),
    );
    gauge(
        &mut out,
        "warlay_db_pool_idle_connections",
        "Idle connections in the PostgreSQL pool.",
        state.db.num_idle(),
    );

    let values = gauges(&state).await;
    gauge(&mut out, "warlay_users", "Accounts.", values.users);
    gauge(
        &mut out,
        "warlay_sessions_active",
        "Open sessions.",
        values.sessions_active,
    );
    gauge(&mut out, "warlay_communities", "Communities.", values.communities);
    gauge(
        &mut out,
        "warlay_reports_open",
        "Player reports waiting for review.",
        values.reports_open,
    );
    gauge(
        &mut out,
        "warlay_users_with_totp",
        "Accounts with two-step verification on.",
        values.users_with_totp,
    );
    gauge(
        &mut out,
        "warlay_google_identities",
        "Accounts linked to Google.",
        values.google_identities,
    );
    header_lines(&mut out, "warlay_rooms", "gauge", "Rooms, by status.");
    for (status, count) in &values.rooms_by_status {
        let _ = writeln!(out, "warlay_rooms{{status=\"{}\"}} {count}", escape(status));
    }
    header_lines(
        &mut out,
        "warlay_helper_active_rooms",
        "gauge",
        "Rooms each helper serves.",
    );
    for h in &values.helpers {
        let _ = writeln!(
            out,
            "warlay_helper_active_rooms{{helper=\"{}\",region=\"{}\"}} {}",
            escape(&h.name),
            escape(&h.region),
            h.active_rooms
        );
    }
    header_lines(
        &mut out,
        "warlay_helper_relay_channels",
        "gauge",
        "Relayed channels each helper carries.",
    );
    for h in &values.helpers {
        let _ = writeln!(
            out,
            "warlay_helper_relay_channels{{helper=\"{}\",region=\"{}\"}} {}",
            escape(&h.name),
            escape(&h.region),
            h.relay_channels
        );
    }
    header_lines(
        &mut out,
        "warlay_helper_clients",
        "gauge",
        "Players connected to each helper's edge.",
    );
    for h in &values.helpers {
        let _ = writeln!(
            out,
            "warlay_helper_clients{{helper=\"{}\",region=\"{}\"}} {}",
            escape(&h.name),
            escape(&h.region),
            h.clients
        );
    }
    state.metrics.render_requests(&mut out);
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        out,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_counted_by_method_route_and_status() {
        let metrics = Metrics::new();
        metrics.observe("GET", "/api/v1/rooms/{id}", 200, Duration::from_millis(3));
        metrics.observe("GET", "/api/v1/rooms/{id}", 200, Duration::from_millis(30));
        metrics.observe("GET", "/api/v1/rooms/{id}", 404, Duration::from_millis(1));
        let mut out = String::new();
        metrics.render_requests(&mut out);
        assert!(
            out.contains("warlay_http_requests_total{method=\"GET\",route=\"/api/v1/rooms/{id}\",status=\"200\"} 2")
        );
        assert!(out.contains("status=\"404\"} 1"));
        // Cumulative buckets: 1 ms and 3 ms are under 5 ms (2 of 3 at 0.005); all 3 by 0.05.
        assert!(out.contains("le=\"0.005\"} 2"), "{out}");
        assert!(out.contains("le=\"0.025\"} 2"), "{out}");
        assert!(out.contains("le=\"0.05\"} 3"), "{out}");
        assert!(out.contains("le=\"+Inf\"} 3"));
        assert!(out.contains("_count{method=\"GET\",route=\"/api/v1/rooms/{id}\"} 3"));
    }

    #[test]
    fn label_values_cannot_break_the_format() {
        assert_eq!(escape("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }

    #[test]
    fn the_number_of_series_is_bounded() {
        let metrics = Metrics::new();
        for i in 0..(MAX_SERIES + 500) {
            metrics.observe("GET", &format!("/r{i}"), 200, Duration::from_millis(1));
        }
        assert_eq!(metrics.requests.lock().unwrap().len(), MAX_SERIES);
        // A series already there keeps counting.
        metrics.observe("GET", "/r0", 200, Duration::from_millis(1));
        assert_eq!(metrics.requests.lock().unwrap()[&("GET".into(), "/r0".into(), 200)], 2);
    }
}
