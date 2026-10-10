use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::models::{load_summaries, pick_color, UserWithPresence};
use crate::realtime::{with_presence, Event};
use crate::routes::rooms::{list_rooms_where, RoomView};
use crate::routes::users::like_prefix;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CommunitySummary {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub accent_color: String,
    pub is_public: bool,
    pub owner_id: Uuid,
    pub member_count: i64,
    /// The caller's role, if they're a member.
    pub my_role: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityMember {
    #[serde(flatten)]
    pub user: UserWithPresence,
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityDetail {
    #[serde(flatten)]
    pub summary: CommunitySummary,
    pub members: Vec<CommunityMember>,
    pub rooms: Vec<RoomView>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCommunity {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub is_public: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct DiscoverQuery {
    #[serde(default)]
    q: String,
}

const SUMMARY_SELECT: &str = "SELECT c.id, c.name, c.slug::text AS slug, c.description, c.accent_color, c.is_public,
        c.owner_id, c.created_at,
        (SELECT count(*) FROM community_members m WHERE m.community_id = c.id) AS member_count,
        (SELECT role FROM community_members m WHERE m.community_id = c.id AND m.user_id = $1) AS my_role
     FROM communities c";

pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        "comunidad".to_string()
    } else {
        slug.chars().take(40).collect()
    }
}

pub async fn role_in(state: &AppState, community_id: Uuid, user_id: Uuid) -> ApiResult<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT role FROM community_members WHERE community_id = $1 AND user_id = $2")
            .bind(community_id)
            .bind(user_id)
            .fetch_optional(&state.db)
            .await?,
    )
}

pub async fn member_ids(state: &AppState, community_id: Uuid) -> ApiResult<Vec<Uuid>> {
    Ok(
        sqlx::query_scalar("SELECT user_id FROM community_members WHERE community_id = $1")
            .bind(community_id)
            .fetch_all(&state.db)
            .await?,
    )
}

async fn load_summary(state: &AppState, me: Uuid, id: Uuid) -> ApiResult<CommunitySummary> {
    sqlx::query_as::<_, CommunitySummary>(&format!("{SUMMARY_SELECT} WHERE c.id = $2"))
        .bind(me)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError::not_found("la comunidad no existe"))
}

pub async fn add_member(state: &AppState, community_id: Uuid, user_id: Uuid, role: &str) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO community_members (community_id, user_id, role) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(community_id)
    .bind(user_id)
    .bind(role)
    .execute(&state.db)
    .await?;
    notify_changed(state, community_id).await
}

async fn notify_changed(state: &AppState, community_id: Uuid) -> ApiResult<()> {
    crate::chat::invalidate(state, &format!("community:{community_id}")).await;
    let members = member_ids(state, community_id).await?;
    state
        .realtime
        .emit(&members, &Event::CommunityUpdated { community_id })
        .await;
    Ok(())
}

pub async fn mine(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<CommunitySummary>>> {
    let rows = sqlx::query_as::<_, CommunitySummary>(&format!(
        "{SUMMARY_SELECT} WHERE EXISTS (SELECT 1 FROM community_members m WHERE m.community_id = c.id AND m.user_id = $1)
         ORDER BY c.name"
    ))
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

pub async fn discover(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<DiscoverQuery>,
) -> ApiResult<Json<Vec<CommunitySummary>>> {
    let q = query.q.trim();
    let rows = if q.is_empty() {
        sqlx::query_as::<_, CommunitySummary>(&format!(
            "{SUMMARY_SELECT} WHERE c.is_public ORDER BY member_count DESC, c.created_at DESC LIMIT 30"
        ))
        .bind(auth.user_id)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, CommunitySummary>(&format!(
            "{SUMMARY_SELECT} WHERE c.is_public AND c.name ILIKE $2 ORDER BY member_count DESC LIMIT 30"
        ))
        .bind(auth.user_id)
        .bind(format!("%{}", like_prefix(q)))
        .fetch_all(&state.db)
        .await?
    };
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateCommunity>,
) -> ApiResult<Json<CommunitySummary>> {
    let name = req.name.trim();
    if name.chars().count() < 3 || name.chars().count() > 40 {
        return Err(ApiError::bad_request(
            "invalid_name",
            "el nombre debe tener entre 3 y 40 caracteres",
        ));
    }
    if req.description.chars().count() > 300 {
        return Err(ApiError::bad_request(
            "description_too_long",
            "la descripción admite hasta 300 caracteres",
        ));
    }
    let id = Uuid::new_v4();
    let base = slugify(name);
    let taken: i64 = sqlx::query_scalar("SELECT count(*) FROM communities WHERE slug = $1")
        .bind(&base)
        .fetch_one(&state.db)
        .await?;
    let slug = if taken == 0 {
        base
    } else {
        format!("{base}-{}", &id.simple().to_string()[..6])
    };
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO communities (id, name, slug, description, accent_color, is_public, owner_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(name)
    .bind(&slug)
    .bind(req.description.trim())
    .bind(pick_color(id))
    .bind(req.is_public)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO community_members (community_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(id)
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(load_summary(&state, auth.user_id, id).await?))
}

pub async fn detail(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<CommunityDetail>> {
    let summary = load_summary(&state, auth.user_id, id).await?;
    if !summary.is_public && summary.my_role.is_none() {
        return Err(ApiError::forbidden("esta comunidad es privada"));
    }
    let roles: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT user_id, role FROM community_members WHERE community_id = $1")
            .bind(id)
            .fetch_all(&state.db)
            .await?;
    let ids: Vec<Uuid> = roles.iter().map(|(u, _)| *u).collect();
    let users = with_presence(&state, load_summaries(&state, &ids).await?).await?;
    let mut members: Vec<CommunityMember> = users
        .into_iter()
        .map(|user| {
            let role = roles
                .iter()
                .find(|(u, _)| *u == user.user.id)
                .map(|(_, r)| r.clone())
                .unwrap_or_else(|| "member".into());
            CommunityMember { user, role }
        })
        .collect();
    members.sort_by_key(|m| match m.role.as_str() {
        "owner" => 0,
        "admin" => 1,
        _ => 2,
    });
    let rooms = list_rooms_where(&state, "r.community_id = $1", Some(id)).await?;
    Ok(Json(CommunityDetail {
        summary,
        members,
        rooms,
    }))
}

pub async fn join(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<CommunitySummary>> {
    let summary = load_summary(&state, auth.user_id, id).await?;
    if !summary.is_public {
        return Err(ApiError::forbidden(
            "esta comunidad solo admite miembros por invitación",
        ));
    }
    add_member(&state, id, auth.user_id, "member").await?;
    Ok(Json(load_summary(&state, auth.user_id, id).await?))
}

pub async fn leave(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    match role_in(&state, id, auth.user_id).await?.as_deref() {
        None => return Err(ApiError::not_found("no eres miembro de esta comunidad")),
        Some("owner") => {
            return Err(ApiError::bad_request(
                "owner_cannot_leave",
                "el dueño no puede salir; elimina la comunidad",
            ))
        }
        Some(_) => {}
    }
    sqlx::query("DELETE FROM community_members WHERE community_id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;
    notify_changed(&state, id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn remove_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let my_role = role_in(&state, id, auth.user_id).await?;
    let their_role = role_in(&state, id, user_id).await?;
    let allowed = matches!(
        (my_role.as_deref(), their_role.as_deref()),
        (Some("owner"), Some("admin" | "member")) | (Some("admin"), Some("member"))
    );
    if !allowed {
        return Err(ApiError::forbidden("no puedes expulsar a ese miembro"));
    }
    sqlx::query("DELETE FROM community_members WHERE community_id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(&state.db)
        .await?;
    state
        .realtime
        .emit(&[user_id], &Event::CommunityUpdated { community_id: id })
        .await;
    notify_changed(&state, id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn delete(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    if role_in(&state, id, auth.user_id).await?.as_deref() != Some("owner") {
        return Err(ApiError::forbidden("solo el dueño puede eliminar la comunidad"));
    }
    crate::chat::invalidate(&state, &format!("community:{id}")).await;
    let members = member_ids(&state, id).await?;
    sqlx::query("DELETE FROM communities WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    state
        .realtime
        .emit(&members, &Event::CommunityUpdated { community_id: id })
        .await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugs_are_url_safe() {
        assert_eq!(slugify("  Los Orcos de Durotar! "), "los-orcos-de-durotar");
        assert_eq!(slugify("¡¡!!"), "comunidad");
    }
}
