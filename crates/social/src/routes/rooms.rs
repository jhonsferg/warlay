use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::{hash_password, verify_password, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::helpers::{helper_by_id, issue_ticket, pick_helper, HelperInfo};
use crate::models::{load_summaries, UserSummary};
use crate::realtime::{broadcast_presence, Event};
use crate::routes::communities::{member_ids, role_in};
use crate::state::AppState;

/// Warcraft III's LAN port, what the host's game listens on by default.
const DEFAULT_GAME_PORT: i32 = 6112;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomView {
    pub id: Uuid,
    pub name: String,
    pub community_id: Option<Uuid>,
    pub community_name: Option<String>,
    pub host: UserSummary,
    pub has_password: bool,
    pub max_players: i32,
    pub game_port: i32,
    pub status: String,
    pub members: Vec<UserSummary>,
    pub created_at: DateTime<Utc>,
    /// When the server read this state (microseconds since the epoch, taken
    /// before the reads, so the state is at least this fresh). Room updates
    /// are coalesced and can reach a client after a newer state it already
    /// has (the reply to its own join): clients drop any older than theirs.
    pub snapshot_at: i64,
    /// The host is disconnected: the room lives on, and they can come back
    /// and manage it.
    pub host_away: bool,
}

/// Everything a member needs to start the P2P session: which helper does
/// rendezvous/relay for this room, and the ticket that lets them in.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomSession {
    pub room: RoomView,
    pub helper: HelperInfo,
    pub ticket: String,
    pub ticket_expires_unix: u64,
    pub is_host: bool,
    pub user_id: Uuid,
}

#[derive(Debug, FromRow)]
struct RoomRow {
    id: Uuid,
    name: String,
    community_id: Option<Uuid>,
    community_name: Option<String>,
    host_id: Uuid,
    has_password: bool,
    max_players: i32,
    game_port: i32,
    status: String,
    created_at: DateTime<Utc>,
    host_away: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoom {
    pub name: String,
    pub community_id: Option<Uuid>,
    pub password: Option<String>,
    pub max_players: Option<i32>,
    pub game_port: Option<i32>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JoinRoom {
    pub password: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetStatus {
    pub status: String,
}

/// Loads full views for `ids`, keeping their order.
pub async fn load_rooms(state: &AppState, ids: &[Uuid]) -> ApiResult<Vec<RoomView>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let snapshot_at = Utc::now().timestamp_micros();
    let rows = sqlx::query_as::<_, RoomRow>(
        "SELECT r.id, r.name, r.community_id, c.name AS community_name, r.host_id,
                r.password_hash IS NOT NULL AS has_password, r.max_players, r.game_port, r.status, r.created_at,
                r.host_away_since IS NOT NULL AS host_away
         FROM rooms r LEFT JOIN communities c ON c.id = r.community_id
         WHERE r.id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(&state.db)
    .await?;
    let members: Vec<(Uuid, Uuid)> =
        sqlx::query_as("SELECT room_id, user_id FROM room_members WHERE room_id = ANY($1) ORDER BY joined_at")
            .bind(ids)
            .fetch_all(&state.db)
            .await?;
    let mut user_ids: Vec<Uuid> = members.iter().map(|(_, u)| *u).collect();
    user_ids.extend(rows.iter().map(|r| r.host_id));
    user_ids.sort();
    user_ids.dedup();
    let users: HashMap<Uuid, UserSummary> = load_summaries(state, &user_ids)
        .await?
        .into_iter()
        .map(|u| (u.id, u))
        .collect();

    let mut views: HashMap<Uuid, RoomView> = HashMap::new();
    for row in rows {
        let Some(host) = users.get(&row.host_id).cloned() else {
            continue;
        };
        let room_members = members
            .iter()
            .filter(|(room, _)| *room == row.id)
            .filter_map(|(_, u)| users.get(u).cloned())
            .collect();
        views.insert(
            row.id,
            RoomView {
                id: row.id,
                name: row.name,
                community_id: row.community_id,
                community_name: row.community_name,
                host,
                has_password: row.has_password,
                max_players: row.max_players,
                game_port: row.game_port,
                status: row.status,
                members: room_members,
                created_at: row.created_at,
                snapshot_at,
                host_away: row.host_away,
            },
        );
    }
    Ok(ids.iter().filter_map(|id| views.remove(id)).collect())
}

/// How many rooms one listing returns; narrow it down with a name search.
const LIST_LIMIT: i64 = 100;

/// Live rooms matching `clause` (which may reference `$1` = `param`) and,
/// if given, whose name contains `name`. Rooms that can be joined come
/// first, newest first.
pub async fn list_rooms_where(
    state: &AppState,
    clause: &str,
    param: Option<Uuid>,
    name: Option<&str>,
) -> ApiResult<Vec<RoomView>> {
    let pattern = name.map(str::trim).filter(|n| !n.is_empty()).map(|n| {
        // ILIKE's escape character is the backslash.
        let escaped = n.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        format!("%{escaped}%")
    });
    let ids: Vec<Uuid> = sqlx::query_scalar(&format!(
        "SELECT r.id FROM rooms r LEFT JOIN communities c ON c.id = r.community_id
         WHERE r.status <> 'closed' AND {clause} AND ($2::text IS NULL OR r.name ILIKE $2)
         ORDER BY (r.status = 'open') DESC, r.created_at DESC LIMIT $3"
    ))
    .bind(param)
    .bind(pattern)
    .bind(LIST_LIMIT)
    .fetch_all(&state.db)
    .await?;
    load_rooms(state, &ids).await
}

async fn load_room(state: &AppState, id: Uuid) -> ApiResult<RoomView> {
    load_rooms(state, &[id])
        .await?
        .pop()
        .filter(|r| r.status != "closed")
        .ok_or_else(|| ApiError::not_found("la sala ya no existe"))
}

pub async fn room_member_ids(state: &AppState, room_id: Uuid) -> ApiResult<Vec<Uuid>> {
    Ok(
        sqlx::query_scalar("SELECT user_id FROM room_members WHERE room_id = $1")
            .bind(room_id)
            .fetch_all(&state.db)
            .await?,
    )
}

/// Who should hear about changes to `room`: its members, plus whoever can
/// see it in a lobby list (its community, or everyone for public rooms).
async fn announce(state: &AppState, room: &RoomView, event: Event) -> ApiResult<()> {
    match room.community_id {
        Some(community_id) => {
            let mut targets = member_ids(state, community_id).await?;
            targets.extend(room.members.iter().map(|m| m.id));
            targets.sort();
            targets.dedup();
            state
                .realtime
                .emit_coalesced(&targets, &event, Some(format!("room:{}", room.id)))
                .await;
        }
        None => state.realtime.emit_all(&event, Some(format!("room:{}", room.id))).await,
    }
    Ok(())
}

async fn announce_updated(state: &AppState, room_id: Uuid) -> ApiResult<RoomView> {
    crate::chat::invalidate(state, &format!("room:{room_id}")).await;
    let room = load_room(state, room_id).await?;
    let json = serde_json::to_value(&room).map_err(ApiError::internal)?;
    announce(state, &room, Event::RoomUpdated { room: json }).await?;
    Ok(room)
}

async fn can_see(state: &AppState, user_id: Uuid, room: &RoomView) -> ApiResult<bool> {
    let Some(community_id) = room.community_id else {
        return Ok(true);
    };
    let public: bool = sqlx::query_scalar("SELECT is_public FROM communities WHERE id = $1")
        .bind(community_id)
        .fetch_optional(&state.db)
        .await?
        .unwrap_or(false);
    Ok(public || role_in(state, community_id, user_id).await?.is_some())
}

/// Builds the session for a member, assigning a live helper if the room's
/// helper went away.
async fn session_for(state: &AppState, room_id: Uuid, user_id: Uuid) -> ApiResult<RoomSession> {
    let room = load_room(state, room_id).await?;
    if !room.members.iter().any(|m| m.id == user_id) {
        return Err(ApiError::forbidden("no eres miembro de esta sala"));
    }
    let helper_id: Option<Uuid> = sqlx::query_scalar("SELECT helper_id FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_one(&state.db)
        .await?;
    let helper = match helper_id {
        Some(id) => helper_by_id(state, id).await?,
        None => None,
    };
    let helper = match helper {
        Some(h) => h,
        None => {
            let h = pick_helper(state).await?;
            sqlx::query("UPDATE rooms SET helper_id = $1 WHERE id = $2")
                .bind(h.id)
                .bind(room_id)
                .execute(&state.db)
                .await?;
            h
        }
    };
    let is_host = room.host.id == user_id;
    let (ticket, ticket_expires_unix) = issue_ticket(state, room_id, user_id, is_host)?;
    Ok(RoomSession {
        room,
        helper,
        ticket,
        ticket_expires_unix,
        is_host,
        user_id,
    })
}

/// The room `user_id` hosts, if any. A host is always a member of it.
async fn hosted_room(state: &AppState, user_id: Uuid) -> ApiResult<Option<Uuid>> {
    Ok(
        sqlx::query_scalar("SELECT id FROM rooms WHERE host_id = $1 AND status <> 'closed'")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await?,
    )
}

/// Joining or creating another room would close the one a user hosts, out
/// from under its players: the host has to close it on purpose first.
async fn ensure_not_hosting(state: &AppState, user_id: Uuid) -> ApiResult<()> {
    if hosted_room(state, user_id).await?.is_some() {
        return Err(ApiError::conflict(
            "hosting",
            "eres el anfitrión de una sala: ciérrala antes de entrar a otra",
        ));
    }
    Ok(())
}

fn host_away_flag(user_id: Uuid) -> String {
    format!("wl:hostaway:{user_id}")
}

/// A user's connection stayed down past the grace period. A player leaves
/// their room; a host doesn't: the room keeps living so the others aren't
/// thrown out, and the host can come back and carry on managing it.
pub async fn disconnect_grace_expired(state: &AppState, user_id: Uuid) -> ApiResult<()> {
    let Some(room_id) = hosted_room(state, user_id).await? else {
        return leave_current_room(state, user_id).await;
    };
    let marked = sqlx::query("UPDATE rooms SET host_away_since = now() WHERE id = $1 AND host_away_since IS NULL")
        .bind(room_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if marked > 0 {
        let mut redis = state.redis.clone();
        let _: Result<(), _> = redis.set_ex(host_away_flag(user_id), 1, 7 * 24 * 3600).await;
        announce_updated(state, room_id).await?;
    }
    Ok(())
}

/// The host connected again: the room is theirs to manage as before.
pub async fn host_came_back(state: &AppState, user_id: Uuid) -> ApiResult<()> {
    let mut redis = state.redis.clone();
    // Only hosts that were marked away have this flag: connecting is hot.
    let was_away: i64 = redis.del(host_away_flag(user_id)).await?;
    if was_away == 0 {
        return Ok(());
    }
    clear_host_away(state, user_id).await
}

async fn clear_host_away(state: &AppState, user_id: Uuid) -> ApiResult<()> {
    let room: Option<Uuid> = sqlx::query_scalar(
        "UPDATE rooms SET host_away_since = NULL
         WHERE host_id = $1 AND host_away_since IS NOT NULL AND status <> 'closed' RETURNING id",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;
    if let Some(room_id) = room {
        announce_updated(state, room_id).await?;
    }
    Ok(())
}

/// Main-server job: closes rooms whose host has been away longer than
/// `host_absence_close_secs`, so abandoned rooms don't pile up. A host that
/// is in fact connected (the marker outlived a lost flag) is cleared, never
/// closed on.
async fn close_abandoned_rooms(state: &AppState) -> ApiResult<()> {
    let mut redis = state.redis.clone();
    let lock: Option<String> = redis::cmd("SET")
        .arg("wl:lock:room-reaper")
        .arg(1)
        .arg("NX")
        .arg("EX")
        .arg(55)
        .query_async(&mut redis)
        .await?;
    if lock.is_none() {
        return Ok(());
    }
    reap_abandoned_rooms(state).await
}

/// One pass of the reaper, without the cluster-wide lock.
pub async fn reap_abandoned_rooms(state: &AppState) -> ApiResult<()> {
    let mut redis = state.redis.clone();
    let abandoned: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, host_id FROM rooms
         WHERE status <> 'closed' AND host_away_since < now() - make_interval(secs => $1)",
    )
    .bind(state.settings.host_absence_close_secs as f64)
    .fetch_all(&state.db)
    .await?;
    for (room_id, host_id) in abandoned {
        let connected: i64 = redis.hlen(crate::realtime::conns_key(host_id)).await.unwrap_or(0);
        if connected > 0 {
            clear_host_away(state, host_id).await?;
        } else {
            tracing::info!(%room_id, %host_id, "closing a room whose host never came back");
            close_room(state, room_id).await?;
        }
    }
    Ok(())
}

/// Removes `user_id` from whatever room they're in; closes it if they host it
/// (an explicit leave: the host asked for it).
pub async fn leave_current_room(state: &AppState, user_id: Uuid) -> ApiResult<()> {
    let room_id: Option<Uuid> = sqlx::query_scalar("SELECT room_id FROM room_members WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await?;
    let Some(room_id) = room_id else {
        return Ok(());
    };
    let host_id: Uuid = sqlx::query_scalar("SELECT host_id FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_one(&state.db)
        .await?;
    if host_id == user_id {
        close_room(state, room_id).await
    } else {
        sqlx::query("DELETE FROM room_members WHERE room_id = $1 AND user_id = $2")
            .bind(room_id)
            .bind(user_id)
            .execute(&state.db)
            .await?;
        announce_updated(state, room_id).await?;
        broadcast_presence(state, user_id).await
    }
}

async fn close_room(state: &AppState, room_id: Uuid) -> ApiResult<()> {
    crate::chat::invalidate(state, &format!("room:{room_id}")).await;
    let room = load_room(state, room_id).await?;
    sqlx::query("UPDATE rooms SET status = 'closed', closed_at = now() WHERE id = $1")
        .bind(room_id)
        .execute(&state.db)
        .await?;
    sqlx::query("DELETE FROM room_members WHERE room_id = $1")
        .bind(room_id)
        .execute(&state.db)
        .await?;
    // `announce` already reaches every member.
    announce(state, &room, Event::RoomClosed { room_id }).await?;
    for member in &room.members {
        broadcast_presence(state, member.id).await?;
    }
    Ok(())
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    /// Part of the room name, case-insensitive.
    pub q: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<Vec<RoomView>>> {
    Ok(Json(
        list_rooms_where(
            &state,
            "(r.community_id IS NULL OR c.is_public OR EXISTS (
                SELECT 1 FROM community_members m WHERE m.community_id = r.community_id AND m.user_id = $1))",
            Some(auth.user_id),
            query.q.as_deref(),
        )
        .await?,
    ))
}

pub async fn current(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Option<RoomSession>>> {
    let room_id: Option<Uuid> = sqlx::query_scalar("SELECT room_id FROM room_members WHERE user_id = $1")
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await?;
    match room_id {
        Some(id) => Ok(Json(Some(session_for(&state, id, auth.user_id).await?))),
        None => Ok(Json(None)),
    }
}

pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateRoom>,
) -> ApiResult<Json<RoomSession>> {
    let name = req.name.trim();
    if name.chars().count() < 3 || name.chars().count() > 48 {
        return Err(ApiError::bad_request(
            "invalid_name",
            "el nombre de la sala debe tener entre 3 y 48 caracteres",
        ));
    }
    let max_players = req.max_players.unwrap_or(12);
    if !(2..=24).contains(&max_players) {
        return Err(ApiError::bad_request("invalid_max_players", "entre 2 y 24 jugadores"));
    }
    let game_port = req.game_port.unwrap_or(DEFAULT_GAME_PORT);
    if !(1024..=65535).contains(&game_port) {
        return Err(ApiError::bad_request("invalid_port", "puerto de juego inválido"));
    }
    if let Some(community_id) = req.community_id {
        if role_in(&state, community_id, auth.user_id).await?.is_none() {
            return Err(ApiError::forbidden(
                "solo los miembros pueden crear salas en esta comunidad",
            ));
        }
    }
    let password_hash = match req.password.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => Some(hash_password(p).await?),
        None => None,
    };
    let helper = pick_helper(&state).await?;
    ensure_not_hosting(&state, auth.user_id).await?;
    leave_current_room(&state, auth.user_id).await?;

    let id = Uuid::new_v4();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO rooms (id, name, community_id, host_id, password_hash, max_players, game_port, status, helper_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'open', $8)",
    )
    .bind(id)
    .bind(name)
    .bind(req.community_id)
    .bind(auth.user_id)
    .bind(password_hash)
    .bind(max_players)
    .bind(game_port)
    .bind(helper.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO room_members (room_id, user_id) VALUES ($1, $2)")
        .bind(id)
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    announce_updated(&state, id).await?;
    broadcast_presence(&state, auth.user_id).await?;
    Ok(Json(session_for(&state, id, auth.user_id).await?))
}

pub async fn detail(State(state): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> ApiResult<Json<RoomView>> {
    let room = load_room(&state, id).await?;
    if !can_see(&state, auth.user_id, &room).await? {
        return Err(ApiError::forbidden("no puedes ver esta sala"));
    }
    Ok(Json(room))
}

/// Joins a room. `invited` skips the password (the invite vouches).
pub async fn join_room(
    state: &AppState,
    user_id: Uuid,
    room_id: Uuid,
    password: Option<&str>,
    invited: bool,
) -> ApiResult<RoomSession> {
    let room = load_room(state, room_id).await?;
    if room.members.iter().any(|m| m.id == user_id) {
        return session_for(state, room_id, user_id).await;
    }
    if !invited && !can_see(state, user_id, &room).await? {
        return Err(ApiError::forbidden("no puedes unirte a esta sala"));
    }
    if room.members.len() as i32 >= room.max_players {
        return Err(ApiError::conflict("room_full", "la sala está llena"));
    }
    if room.has_password && !invited {
        let hash: Option<String> = sqlx::query_scalar("SELECT password_hash FROM rooms WHERE id = $1")
            .bind(room_id)
            .fetch_one(&state.db)
            .await?;
        let ok = match (hash, password) {
            (Some(h), Some(p)) => verify_password(p, Some(&h)).await?,
            _ => false,
        };
        if !ok {
            return Err(ApiError::new(
                axum::http::StatusCode::FORBIDDEN,
                "wrong_password",
                "contraseña de sala incorrecta",
            ));
        }
    }
    ensure_not_hosting(state, user_id).await?;
    leave_current_room(state, user_id).await?;
    // Capacity and status are checked again under the room's row lock:
    // simultaneous joins otherwise all pass the check above and overfill it,
    // or land in a room being closed.
    let mut tx = state.db.begin().await?;
    let locked: Option<(i32, String)> =
        sqlx::query_as("SELECT max_players, status FROM rooms WHERE id = $1 FOR UPDATE")
            .bind(room_id)
            .fetch_optional(&mut *tx)
            .await?;
    // A running game doesn't close the room to newcomers: the room is a
    // virtual LAN, and `in_game` is only a badge.
    let Some((max_players, _)) = locked.filter(|(_, s)| s != "closed") else {
        return Err(ApiError::not_found("la sala ya no existe"));
    };
    let members: i64 = sqlx::query_scalar("SELECT count(*) FROM room_members WHERE room_id = $1")
        .bind(room_id)
        .fetch_one(&mut *tx)
        .await?;
    if members >= i64::from(max_players) {
        return Err(ApiError::conflict("room_full", "la sala está llena"));
    }
    sqlx::query("INSERT INTO room_members (room_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(room_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    announce_updated(state, room_id).await?;
    broadcast_presence(state, user_id).await?;
    session_for(state, room_id, user_id).await
}

pub async fn join(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    body: Option<Json<JoinRoom>>,
) -> ApiResult<Json<RoomSession>> {
    let req = body.map(|Json(b)| b).unwrap_or_default();
    Ok(Json(
        join_room(&state, auth.user_id, id, req.password.as_deref(), false).await?,
    ))
}

pub async fn session(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RoomSession>> {
    Ok(Json(session_for(&state, id, auth.user_id).await?))
}

pub async fn leave(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let member: Option<Uuid> =
        sqlx::query_scalar("SELECT room_id FROM room_members WHERE user_id = $1 AND room_id = $2")
            .bind(auth.user_id)
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    if member.is_none() {
        return Err(ApiError::not_found("no estás en esa sala"));
    }
    leave_current_room(&state, auth.user_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn require_host(state: &AppState, room_id: Uuid, user_id: Uuid) -> ApiResult<RoomView> {
    let room = load_room(state, room_id).await?;
    if room.host.id != user_id {
        return Err(ApiError::forbidden("solo el anfitrión puede hacer esto"));
    }
    Ok(room)
}

pub async fn set_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SetStatus>,
) -> ApiResult<Json<RoomView>> {
    let room = require_host(&state, id, auth.user_id).await?;
    if !matches!(req.status.as_str(), "open" | "in_game") {
        return Err(ApiError::bad_request("invalid_status", "estado inválido"));
    }
    sqlx::query("UPDATE rooms SET status = $1 WHERE id = $2")
        .bind(&req.status)
        .bind(id)
        .execute(&state.db)
        .await?;
    let updated = announce_updated(&state, id).await?;
    for member in &room.members {
        broadcast_presence(&state, member.id).await?;
    }
    Ok(Json(updated))
}

pub async fn kick(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    require_host(&state, id, auth.user_id).await?;
    if user_id == auth.user_id {
        return Err(ApiError::bad_request("self_kick", "no puedes expulsarte a ti mismo"));
    }
    let removed = sqlx::query("DELETE FROM room_members WHERE room_id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if removed == 0 {
        return Err(ApiError::not_found("ese jugador no está en la sala"));
    }
    state
        .realtime
        .emit(&[user_id], &Event::RoomClosed { room_id: id })
        .await;
    announce_updated(&state, id).await?;
    broadcast_presence(&state, user_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn delete(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    require_host(&state, id, auth.user_id).await?;
    close_room(&state, id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Main-server background job: rooms whose helper stopped heartbeating are
/// handed to a live one, and their members told to move. A Redis lock keeps
/// it to one main instance at a time.
pub fn spawn_helper_watchdog(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            tick.tick().await;
            if let Err(error) = reassign_orphaned_rooms(&state).await {
                tracing::warn!(?error, "helper watchdog failed");
            }
            if let Err(error) = close_abandoned_rooms(&state).await {
                tracing::warn!(?error, "abandoned room reaper failed");
            }
        }
    });
}

async fn reassign_orphaned_rooms(state: &AppState) -> ApiResult<()> {
    let mut redis = state.redis.clone();
    let lock: Option<String> = redis::cmd("SET")
        .arg("wl:lock:helper-watchdog")
        .arg(state.realtime.node_id.to_string())
        .arg("NX")
        .arg("EX")
        .arg(4)
        .query_async(&mut redis)
        .await?;
    if lock.is_none() {
        return Ok(());
    }
    let live: Vec<Uuid> = crate::helpers::live_helpers(state)
        .await?
        .into_iter()
        .map(|h| h.helper_id)
        .collect();
    if live.is_empty() {
        return Ok(());
    }
    let orphaned: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM rooms WHERE status <> 'closed' AND (helper_id IS NULL OR NOT (helper_id = ANY($1)))",
    )
    .bind(&live)
    .fetch_all(&state.db)
    .await?;
    for room_id in orphaned {
        let helper = pick_helper(state).await?;
        sqlx::query("UPDATE rooms SET helper_id = $1 WHERE id = $2")
            .bind(helper.id)
            .bind(room_id)
            .execute(&state.db)
            .await?;
        let members = room_member_ids(state, room_id).await?;
        tracing::info!(%room_id, helper = %helper.name, members = members.len(), "room moved to a live helper");
        state
            .realtime
            .emit(&members, &Event::RoomHelperChanged { room_id, helper })
            .await;
    }
    // Release it only if it's still ours: past its expiry another instance
    // may already hold it.
    let _: Result<i64, _> = redis::cmd("EVAL")
        .arg("if redis.call('GET', KEYS[1]) == ARGV[1] then return redis.call('DEL', KEYS[1]) else return 0 end")
        .arg(1)
        .arg("wl:lock:helper-watchdog")
        .arg(state.realtime.node_id.to_string())
        .query_async(&mut redis)
        .await;
    Ok(())
}
