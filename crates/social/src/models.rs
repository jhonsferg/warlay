use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::state::AppState;

/// Palette new accounts and communities pick their color from.
pub const AVATAR_COLORS: &[&str] = &[
    "#5865F2", "#3BA55C", "#FAA61A", "#ED4245", "#EB459E", "#9B59B6", "#1ABC9C", "#E67E22", "#3498DB", "#F1C40F",
];

pub fn pick_color(seed: Uuid) -> String {
    let index = (seed.as_u128() % AVATAR_COLORS.len() as u128) as usize;
    AVATAR_COLORS[index].to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceStatus {
    Offline,
    Online,
    InRoom,
    InGame,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    pub status: PresenceStatus,
    pub room_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSummary {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_color: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub avatar_color: String,
    pub bio: String,
    pub created_at: DateTime<Utc>,
}

impl UserRow {
    pub fn summary(&self) -> UserSummary {
        UserSummary {
            id: self.id,
            username: self.username.clone(),
            display_name: self.display_name.clone(),
            avatar_color: self.avatar_color.clone(),
        }
    }
}

/// The caller's own account.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub display_name: String,
    pub avatar_color: String,
    pub bio: String,
    pub created_at: DateTime<Utc>,
}

impl From<UserRow> for Me {
    fn from(u: UserRow) -> Self {
        Self {
            id: u.id,
            username: u.username,
            email: u.email,
            display_name: u.display_name,
            avatar_color: u.avatar_color,
            bio: u.bio,
            created_at: u.created_at,
        }
    }
}

/// A user plus their live presence, as shown in friend and member lists.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserWithPresence {
    #[serde(flatten)]
    pub user: UserSummary,
    pub presence: Presence,
}

#[derive(Debug, Clone, FromRow)]
pub struct SummaryRow {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_color: String,
}

impl From<SummaryRow> for UserSummary {
    fn from(r: SummaryRow) -> Self {
        Self {
            id: r.id,
            username: r.username,
            display_name: r.display_name,
            avatar_color: r.avatar_color,
        }
    }
}

pub const SUMMARY_COLUMNS: &str = "u.id, u.username::text AS username, u.display_name, u.avatar_color";

pub async fn load_user(state: &AppState, id: Uuid) -> ApiResult<Option<UserRow>> {
    Ok(sqlx::query_as::<_, UserRow>(
        "SELECT id, username::text AS username, email::text AS email, password_hash, display_name, avatar_color, bio, created_at
         FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?)
}

pub async fn load_summaries(state: &AppState, ids: &[Uuid]) -> ApiResult<Vec<UserSummary>> {
    let rows = sqlx::query_as::<_, SummaryRow>(&format!(
        "SELECT {SUMMARY_COLUMNS} FROM users u WHERE u.id = ANY($1) ORDER BY u.display_name"
    ))
    .bind(ids)
    .fetch_all(&state.db)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Ids of everyone `user_id` is friends with.
pub async fn friend_ids(state: &AppState, user_id: Uuid) -> ApiResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar::<_, Uuid>(
        "SELECT CASE WHEN user_a = $1 THEN user_b ELSE user_a END FROM friendships WHERE user_a = $1 OR user_b = $1",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?)
}

pub async fn are_friends(state: &AppState, a: Uuid, b: Uuid) -> ApiResult<bool> {
    let (low, high) = (a.min(b), a.max(b));
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM friendships WHERE user_a = $1 AND user_b = $2")
            .bind(low)
            .bind(high)
            .fetch_one(&state.db)
            .await?
            > 0,
    )
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: Uuid,
    pub kind: String,
    pub payload: serde_json::Value,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct NotificationRow {
    pub id: Uuid,
    pub kind: String,
    pub payload: serde_json::Value,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<NotificationRow> for Notification {
    fn from(r: NotificationRow) -> Self {
        Self {
            id: r.id,
            kind: r.kind,
            payload: r.payload,
            read: r.read_at.is_some(),
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: Uuid,
    pub channel: String,
    pub sender: UserSummary,
    pub body: String,
    pub created_at: DateTime<Utc>,
}
