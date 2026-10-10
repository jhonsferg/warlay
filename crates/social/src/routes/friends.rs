use axum::extract::{Path, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::models::{are_friends, friend_ids, load_summaries, load_user, UserSummary, UserWithPresence};
use crate::realtime::{with_presence, Event};
use crate::routes::notifications::notify;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendRequest {
    pub id: Uuid,
    pub from: UserSummary,
    pub to: UserSummary,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendRequests {
    pub incoming: Vec<FriendRequest>,
    pub outgoing: Vec<FriendRequest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequest {
    pub username: Option<String>,
    pub user_id: Option<Uuid>,
}

pub async fn list(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<UserWithPresence>>> {
    let ids = friend_ids(&state, auth.user_id).await?;
    let users = load_summaries(&state, &ids).await?;
    Ok(Json(with_presence(&state, users).await?))
}

async fn load_request(state: &AppState, id: Uuid) -> ApiResult<Option<FriendRequest>> {
    let row: Option<(Uuid, Uuid, DateTime<Utc>)> =
        sqlx::query_as("SELECT requester_id, addressee_id, created_at FROM friend_requests WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let Some((from, to, created_at)) = row else {
        return Ok(None);
    };
    let users = load_summaries(state, &[from, to]).await?;
    let find = |id: Uuid| users.iter().find(|u| u.id == id).cloned();
    Ok(match (find(from), find(to)) {
        (Some(from), Some(to)) => Some(FriendRequest {
            id,
            from,
            to,
            created_at,
        }),
        _ => None,
    })
}

pub async fn requests(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<FriendRequests>> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM friend_requests WHERE requester_id = $1 OR addressee_id = $1 ORDER BY created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    let mut incoming = Vec::new();
    let mut outgoing = Vec::new();
    for id in ids {
        if let Some(request) = load_request(&state, id).await? {
            if request.to.id == auth.user_id {
                incoming.push(request);
            } else {
                outgoing.push(request);
            }
        }
    }
    Ok(Json(FriendRequests { incoming, outgoing }))
}

pub async fn send_request(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<SendRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let target: Option<Uuid> = match (&req.user_id, &req.username) {
        (Some(id), _) => Some(*id),
        (None, Some(username)) => {
            sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
                .bind(username.trim())
                .fetch_optional(&state.db)
                .await?
        }
        (None, None) => None,
    };
    let not_found = || ApiError::not_found("no existe un usuario con ese nombre");
    let target = target.ok_or_else(not_found)?;
    if target == auth.user_id {
        return Err(ApiError::bad_request("self_request", "no puedes agregarte a ti mismo"));
    }
    load_user(&state, target).await?.ok_or_else(not_found)?;
    if are_friends(&state, auth.user_id, target).await? {
        return Err(ApiError::conflict("already_friends", "ya son amigos"));
    }

    // If they already asked us, asking back means accepting.
    let reverse: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM friend_requests WHERE requester_id = $1 AND addressee_id = $2")
            .bind(target)
            .bind(auth.user_id)
            .fetch_optional(&state.db)
            .await?;
    if let Some(reverse_id) = reverse {
        accept_request(&state, auth.user_id, reverse_id).await?;
        return Ok(Json(serde_json::json!({ "status": "accepted" })));
    }

    let id = Uuid::new_v4();
    let inserted = sqlx::query("INSERT INTO friend_requests (id, requester_id, addressee_id) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(auth.user_id)
        .bind(target)
        .execute(&state.db)
        .await;
    if let Err(sqlx::Error::Database(e)) = &inserted {
        if e.is_unique_violation() {
            return Err(ApiError::conflict(
                "already_requested",
                "ya enviaste una solicitud a ese usuario",
            ));
        }
    }
    inserted?;

    if let Some(request) = load_request(&state, id).await? {
        let json = serde_json::to_value(&request).map_err(ApiError::internal)?;
        state
            .realtime
            .emit(&[target, auth.user_id], &Event::FriendRequest { request: json.clone() })
            .await;
        notify(&state, target, "friend_request", json).await?;
    }
    Ok(Json(serde_json::json!({ "status": "pending", "id": id })))
}

async fn accept_request(state: &AppState, me: Uuid, request_id: Uuid) -> ApiResult<()> {
    let request = load_request(state, request_id)
        .await?
        .ok_or_else(|| ApiError::not_found("la solicitud ya no existe"))?;
    if request.to.id != me {
        return Err(ApiError::forbidden("esa solicitud no es para ti"));
    }
    let (from, to) = (request.from.id, request.to.id);
    crate::chat::invalidate(state, &crate::chat::dm_key(from, to)).await;
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "DELETE FROM friend_requests
         WHERE (requester_id = $1 AND addressee_id = $2) OR (requester_id = $2 AND addressee_id = $1)",
    )
    .bind(from)
    .bind(to)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO friendships (user_a, user_b) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(from.min(to))
        .bind(from.max(to))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let users = with_presence(state, vec![request.from.clone(), request.to.clone()]).await?;
    for (target, friend_id) in [(from, to), (to, from)] {
        if let Some(friend) = users.iter().find(|u| u.user.id == friend_id).cloned() {
            state.realtime.emit(&[target], &Event::FriendAdded { friend }).await;
        }
    }
    state
        .realtime
        .emit(
            &[from, to],
            &Event::FriendRequest {
                request: serde_json::json!({ "id": request_id, "removed": true }),
            },
        )
        .await;
    notify(
        state,
        from,
        "friend_accepted",
        serde_json::json!({ "user": request.to }),
    )
    .await?;
    Ok(())
}

pub async fn accept(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    accept_request(&state, auth.user_id, id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Declines an incoming request, or cancels an outgoing one.
pub async fn decline(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let request = load_request(&state, id)
        .await?
        .ok_or_else(|| ApiError::not_found("la solicitud ya no existe"))?;
    if request.from.id != auth.user_id && request.to.id != auth.user_id {
        return Err(ApiError::forbidden("esa solicitud no es tuya"));
    }
    sqlx::query("DELETE FROM friend_requests WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    state
        .realtime
        .emit(
            &[request.from.id, request.to.id],
            &Event::FriendRequest {
                request: serde_json::json!({ "id": id, "removed": true }),
            },
        )
        .await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn remove(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(friend_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let removed = sqlx::query("DELETE FROM friendships WHERE user_a = $1 AND user_b = $2")
        .bind(auth.user_id.min(friend_id))
        .bind(auth.user_id.max(friend_id))
        .execute(&state.db)
        .await?
        .rows_affected();
    if removed == 0 {
        return Err(ApiError::not_found("no son amigos"));
    }
    crate::chat::invalidate(&state, &crate::chat::dm_key(auth.user_id, friend_id)).await;
    state
        .realtime
        .emit(&[auth.user_id], &Event::FriendRemoved { user_id: friend_id })
        .await;
    state
        .realtime
        .emit(&[friend_id], &Event::FriendRemoved { user_id: auth.user_id })
        .await;
    Ok(Json(serde_json::json!({ "ok": true })))
}
