use axum::extract::{Path, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::models::{are_friends, load_summaries, UserSummary};
use crate::realtime::Event;
use crate::routes::communities::{add_member, role_in};
use crate::routes::notifications::notify;
use crate::routes::rooms::{join_room, room_member_ids};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteView {
    pub id: Uuid,
    pub kind: String,
    pub target_id: Uuid,
    pub target_name: String,
    pub inviter: UserSummary,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteRequest {
    pub user_id: Uuid,
}

#[derive(FromRow)]
struct InviteRow {
    id: Uuid,
    kind: String,
    target_id: Uuid,
    inviter_id: Uuid,
    invitee_id: Uuid,
    status: String,
    created_at: DateTime<Utc>,
}

async fn target_name(state: &AppState, kind: &str, target_id: Uuid) -> ApiResult<String> {
    let sql = match kind {
        "community" => "SELECT name FROM communities WHERE id = $1",
        _ => "SELECT name FROM rooms WHERE id = $1",
    };
    Ok(sqlx::query_scalar::<_, String>(sql)
        .bind(target_id)
        .fetch_optional(&state.db)
        .await?
        .unwrap_or_default())
}

async fn view(state: &AppState, row: &InviteRow) -> ApiResult<Option<InviteView>> {
    let Some(inviter) = load_summaries(state, &[row.inviter_id]).await?.pop() else {
        return Ok(None);
    };
    Ok(Some(InviteView {
        id: row.id,
        kind: row.kind.clone(),
        target_id: row.target_id,
        target_name: target_name(state, &row.kind, row.target_id).await?,
        inviter,
        created_at: row.created_at,
    }))
}

async fn create_invite(
    state: &AppState,
    kind: &str,
    target_id: Uuid,
    inviter: Uuid,
    invitee: Uuid,
) -> ApiResult<InviteView> {
    if inviter == invitee {
        return Err(ApiError::bad_request("self_invite", "no puedes invitarte a ti mismo"));
    }
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM invites WHERE kind = $1 AND target_id = $2 AND invitee_id = $3 AND status = 'pending'",
    )
    .bind(kind)
    .bind(target_id)
    .bind(invitee)
    .fetch_optional(&state.db)
    .await?;
    if existing.is_some() {
        return Err(ApiError::conflict(
            "already_invited",
            "ya tiene una invitación pendiente",
        ));
    }
    let row = sqlx::query_as::<_, InviteRow>(
        "INSERT INTO invites (id, kind, target_id, inviter_id, invitee_id, status)
         VALUES ($1, $2, $3, $4, $5, 'pending')
         RETURNING id, kind, target_id, inviter_id, invitee_id, status, created_at",
    )
    .bind(Uuid::new_v4())
    .bind(kind)
    .bind(target_id)
    .bind(inviter)
    .bind(invitee)
    .fetch_one(&state.db)
    .await?;
    let invite = view(state, &row)
        .await?
        .ok_or_else(|| ApiError::internal("inviter vanished"))?;
    let json = serde_json::to_value(&invite).map_err(ApiError::internal)?;
    state
        .realtime
        .emit(&[invitee], &Event::Invite { invite: json.clone() })
        .await;
    notify(state, invitee, &format!("{kind}_invite"), json).await?;
    Ok(invite)
}

pub async fn invite_to_community(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<InviteRequest>,
) -> ApiResult<Json<InviteView>> {
    if role_in(&state, id, auth.user_id).await?.is_none() {
        return Err(ApiError::forbidden("solo los miembros pueden invitar"));
    }
    if role_in(&state, id, req.user_id).await?.is_some() {
        return Err(ApiError::conflict("already_member", "ya es miembro de la comunidad"));
    }
    Ok(Json(
        create_invite(&state, "community", id, auth.user_id, req.user_id).await?,
    ))
}

pub async fn invite_to_room(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<InviteRequest>,
) -> ApiResult<Json<InviteView>> {
    let members = room_member_ids(&state, id).await?;
    if !members.contains(&auth.user_id) {
        return Err(ApiError::forbidden("solo los jugadores de la sala pueden invitar"));
    }
    if members.contains(&req.user_id) {
        return Err(ApiError::conflict("already_member", "ya está en la sala"));
    }
    if !are_friends(&state, auth.user_id, req.user_id).await? {
        return Err(ApiError::forbidden("solo puedes invitar a tus amigos"));
    }
    Ok(Json(
        create_invite(&state, "room", id, auth.user_id, req.user_id).await?,
    ))
}

pub async fn pending(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<InviteView>>> {
    let rows = sqlx::query_as::<_, InviteRow>(
        "SELECT id, kind, target_id, inviter_id, invitee_id, status, created_at FROM invites
         WHERE invitee_id = $1 AND status = 'pending' ORDER BY created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    let mut views = Vec::new();
    for row in &rows {
        if let Some(v) = view(&state, row).await? {
            views.push(v);
        }
    }
    Ok(Json(views))
}

async fn own_pending(state: &AppState, user_id: Uuid, id: Uuid) -> ApiResult<InviteRow> {
    let row = sqlx::query_as::<_, InviteRow>(
        "SELECT id, kind, target_id, inviter_id, invitee_id, status, created_at FROM invites WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .filter(|r| r.invitee_id == user_id && r.status == "pending")
    .ok_or_else(|| ApiError::not_found("la invitación ya no está disponible"))?;
    Ok(row)
}

async fn set_status(state: &AppState, id: Uuid, status: &str) -> ApiResult<()> {
    sqlx::query("UPDATE invites SET status = $1 WHERE id = $2")
        .bind(status)
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Accepting a community invite makes you a member; a room invite puts
/// you in the room (and returns its session, like joining).
pub async fn accept(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let row = own_pending(&state, auth.user_id, id).await?;
    let result = match row.kind.as_str() {
        "community" => {
            add_member(&state, row.target_id, auth.user_id, "member").await?;
            serde_json::json!({ "kind": "community", "communityId": row.target_id })
        }
        _ => {
            let session = join_room(&state, auth.user_id, row.target_id, None, true).await?;
            serde_json::json!({ "kind": "room", "session": session })
        }
    };
    set_status(&state, id, "accepted").await?;
    Ok(Json(result))
}

pub async fn decline(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    own_pending(&state, auth.user_id, id).await?;
    set_status(&state, id, "declined").await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
