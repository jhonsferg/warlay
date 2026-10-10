use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::models::{are_friends, load_user, SummaryRow, UserWithPresence, SUMMARY_COLUMNS};
use crate::realtime::with_presence;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
}

/// Escapes LIKE wildcards so user input only ever matches literally.
pub fn like_prefix(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len() + 1);
    for c in input.chars() {
        if matches!(c, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped.push('%');
    escaped
}

/// Prefix search on username / display name, for adding friends.
pub async fn search(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Json<Vec<UserWithPresence>>> {
    let q = query.q.trim();
    if q.chars().count() < 2 {
        return Ok(Json(Vec::new()));
    }
    let rows = sqlx::query_as::<_, SummaryRow>(&format!(
        "SELECT {SUMMARY_COLUMNS} FROM users u
         WHERE u.id <> $1 AND (u.username ILIKE $2 OR u.display_name ILIKE $2)
         ORDER BY u.username LIMIT 20"
    ))
    .bind(auth.user_id)
    .bind(like_prefix(q))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        with_presence(&state, rows.into_iter().map(Into::into).collect()).await?,
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    #[serde(flatten)]
    pub user: UserWithPresence,
    pub bio: String,
    pub member_since: chrono::DateTime<chrono::Utc>,
    pub is_friend: bool,
}

pub async fn profile(State(state): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> ApiResult<Json<Profile>> {
    let user = load_user(&state, id)
        .await?
        .ok_or_else(|| ApiError::not_found("usuario no encontrado"))?;
    let is_friend = are_friends(&state, auth.user_id, id).await?;
    let mut with = with_presence(&state, vec![user.summary()]).await?;
    Ok(Json(Profile {
        user: with.remove(0),
        bio: user.bio,
        member_since: user.created_at,
        is_friend,
    }))
}

#[cfg(test)]
mod tests {
    use super::like_prefix;

    #[test]
    fn like_prefix_escapes_wildcards() {
        assert_eq!(like_prefix("jh"), "jh%");
        assert_eq!(like_prefix("a_%"), "a\\_\\%%");
    }
}
