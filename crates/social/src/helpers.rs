use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use base64::Engine;
use redis::AsyncCommands;
use serde::Serialize;
use uuid::Uuid;
use warlay_protocol::{HelperHeartbeat, RoomTicket};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// A helper missing heartbeats for this long is considered gone.
const HELPER_TTL_SECS: u64 = 12;
const HELPERS_SET: &str = "wl:helpers";

fn helper_key(id: Uuid) -> String {
    format!("wl:helper:{id}")
}

/// What a player needs to reach the helper assigned to its room.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperInfo {
    pub id: Uuid,
    pub name: String,
    pub region: String,
    pub rendezvous_addr: String,
    pub relay_addr: String,
    /// Where players connect for realtime events and chat (may be empty).
    pub edge_url: String,
}

impl From<HelperHeartbeat> for HelperInfo {
    fn from(h: HelperHeartbeat) -> Self {
        Self {
            id: h.helper_id,
            name: h.name,
            region: h.region,
            rendezvous_addr: h.rendezvous_addr,
            relay_addr: h.relay_addr,
            edge_url: h.edge_url,
        }
    }
}

/// Records a helper's heartbeat (also usable in-process by an embedded helper).
pub async fn record_heartbeat(state: &AppState, heartbeat: &HelperHeartbeat) -> ApiResult<()> {
    let mut redis = state.redis.clone();
    let json = serde_json::to_string(heartbeat).map_err(ApiError::internal)?;
    let _: () = redis
        .set_ex(helper_key(heartbeat.helper_id), json, HELPER_TTL_SECS)
        .await?;
    let _: () = redis.sadd(HELPERS_SET, heartbeat.helper_id.to_string()).await?;
    Ok(())
}

/// `POST /internal/helpers/heartbeat`, authenticated with the shared secret.
pub async fn heartbeat_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(heartbeat): Json<HelperHeartbeat>,
) -> ApiResult<Json<serde_json::Value>> {
    let presented = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    let secret = state.settings.helper_secret.as_bytes();
    if secret.is_empty() || !constant_time_eq::constant_time_eq(presented.as_bytes(), secret) {
        return Err(ApiError::unauthorized());
    }
    record_heartbeat(&state, &heartbeat).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn live_helpers(state: &AppState) -> ApiResult<Vec<HelperHeartbeat>> {
    let mut redis = state.redis.clone();
    let ids: Vec<String> = redis.smembers(HELPERS_SET).await?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let keys: Vec<String> = ids
        .iter()
        .filter_map(|id| Uuid::parse_str(id).ok())
        .map(helper_key)
        .collect();
    let values: Vec<Option<String>> = redis.mget(&keys).await?;
    let mut helpers = Vec::new();
    for (id, value) in ids.iter().zip(values) {
        match value.and_then(|v| serde_json::from_str::<HelperHeartbeat>(&v).ok()) {
            Some(h) => helpers.push(h),
            None => {
                let _: () = redis.srem(HELPERS_SET, id).await?;
            }
        }
    }
    Ok(helpers)
}

pub async fn helper_by_id(state: &AppState, id: Uuid) -> ApiResult<Option<HelperInfo>> {
    let mut redis = state.redis.clone();
    let value: Option<String> = redis.get(helper_key(id)).await?;
    Ok(value
        .and_then(|v| serde_json::from_str::<HelperHeartbeat>(&v).ok())
        .map(Into::into))
}

/// The least loaded live helper.
pub async fn pick_helper(state: &AppState) -> ApiResult<HelperInfo> {
    let helpers = live_helpers(state).await?;
    let recent = recent_assignments(state, "room", &helpers).await?;
    let loads: Vec<u64> = helpers
        .iter()
        .zip(&recent)
        .map(|(h, r)| u64::from(h.active_rooms) * 4 + u64::from(h.relay_channels) * 8 + u64::from(h.clients) + r * 4)
        .collect();
    let chosen = power_of_two(&loads);
    if let Some(i) = chosen {
        note_assignment(state, "room", helpers[i].helper_id).await?;
    }
    chosen.map(|i| helpers[i].clone().into()).ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "no_helpers",
            "no hay servidores de emparejamiento disponibles",
        )
    })
}

/// Signs a ticket letting `user_id` into `room_id`'s rendezvous and relay.
pub fn issue_ticket(state: &AppState, room_id: Uuid, user_id: Uuid, is_host: bool) -> ApiResult<(String, u64)> {
    let expires_unix = chrono::Utc::now().timestamp() as u64 + state.settings.room_ticket_ttl_secs;
    let ticket = RoomTicket {
        room_id,
        user_id,
        is_host,
        expires_unix,
    };
    let payload = ticket.encode().map_err(ApiError::internal)?;
    let signed = warlay_crypto::sign_token(state.settings.helper_secret.as_bytes(), &payload);
    Ok((base64::engine::general_purpose::STANDARD.encode(signed), expires_unix))
}

/// One edge a player can connect to, in order of preference.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeInfo {
    pub id: Uuid,
    pub name: String,
    pub region: String,
    pub url: String,
    pub clients: u32,
}

/// `GET /api/v1/edges` -- live helper edges, least loaded first. Players
/// keep this list so they can move to another edge (even while the main
/// server is down) if theirs disappears.
pub async fn edges_handler(State(state): State<AppState>) -> ApiResult<Json<Vec<EdgeInfo>>> {
    let helpers: Vec<HelperHeartbeat> = live_helpers(&state)
        .await?
        .into_iter()
        .filter(|h| !h.edge_url.is_empty())
        .collect();
    let recent = recent_assignments(&state, "edge", &helpers).await?;
    let loads: Vec<u64> = helpers
        .iter()
        .zip(&recent)
        .map(|(h, r)| u64::from(h.clients) + r)
        .collect();
    let chosen = power_of_two(&loads);
    if let Some(i) = chosen {
        note_assignment(&state, "edge", helpers[i].helper_id).await?;
    }
    let mut ranked: Vec<(u64, HelperHeartbeat)> = loads.into_iter().zip(helpers).collect();
    // The power-of-two pick first, then the rest from least to most loaded.
    let first = chosen.map(|i| ranked.remove(i));
    ranked.sort_by_key(|(load, _)| *load);
    let edges: Vec<EdgeInfo> = first
        .into_iter()
        .chain(ranked)
        .map(|(_, h)| h)
        .map(|h| EdgeInfo {
            id: h.helper_id,
            name: h.name,
            region: h.region,
            url: h.edge_url,
            clients: h.clients,
        })
        .collect();
    Ok(Json(edges))
}

/// "Power of two choices": compare two random candidates and keep the less
/// loaded one. Nearly as good as always taking the global minimum, but a
/// burst of requests (hundreds of players logging in at once, all seeing
/// the same 10-second-old loads) doesn't pile onto a single node.
fn power_of_two(loads: &[u64]) -> Option<usize> {
    match loads.len() {
        0 => None,
        1 => Some(0),
        n => {
            let r = Uuid::new_v4().as_u128();
            let a = (r % n as u128) as usize;
            let mut b = ((r >> 64) % (n as u128 - 1)) as usize;
            if b >= a {
                b += 1;
            }
            Some(if loads[b] < loads[a] { b } else { a })
        }
    }
}

/// Assignments handed out in the last seconds, which the helpers'
/// heartbeats don't reflect yet.
async fn recent_assignments(state: &AppState, kind: &str, helpers: &[HelperHeartbeat]) -> ApiResult<Vec<u64>> {
    if helpers.is_empty() {
        return Ok(Vec::new());
    }
    let keys: Vec<String> = helpers
        .iter()
        .map(|h| format!("wl:assign:{kind}:{}", h.helper_id))
        .collect();
    let mut redis = state.redis.clone();
    let values: Vec<Option<u64>> = redis.mget(&keys).await?;
    Ok(values.into_iter().map(|v| v.unwrap_or(0)).collect())
}

async fn note_assignment(state: &AppState, kind: &str, helper_id: Uuid) -> ApiResult<()> {
    let key = format!("wl:assign:{kind}:{helper_id}");
    let mut redis = state.redis.clone();
    let _: () = redis::pipe()
        .incr(&key, 1)
        .ignore()
        .expire(&key, 20)
        .ignore()
        .query_async(&mut redis)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::power_of_two;

    #[test]
    fn power_of_two_prefers_the_lighter_node_and_spreads_bursts() {
        assert_eq!(power_of_two(&[]), None);
        assert_eq!(power_of_two(&[7]), Some(0));
        // Two nodes: always the lighter one.
        for _ in 0..50 {
            assert_eq!(power_of_two(&[10, 3]), Some(1));
        }
        // Equal loads: every node gets picked over many draws.
        let mut hits = [0; 4];
        for _ in 0..400 {
            hits[power_of_two(&[5, 5, 5, 5]).unwrap()] += 1;
        }
        assert!(hits.iter().all(|&h| h > 40), "{hits:?}");
    }
}
