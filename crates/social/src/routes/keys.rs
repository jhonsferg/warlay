//! The directory of public keys for end-to-end encrypted direct messages.
//! The server only ever holds public keys: private ones never leave the
//! device, and the messages themselves are opaque to it.

use axum::extract::{Path, State};
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{rate_limit, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::models::are_friends;
use crate::state::AppState;

const KEY_LEN: usize = 32;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishKey {
    /// 32-byte X25519 public key, base64url without padding.
    public_key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyView {
    pub public_key: String,
    pub updated_at: DateTime<Utc>,
    /// Whether this replaced a different key (a new device, or a reset):
    /// friends who pinned the old one must be asked to trust the new one.
    pub changed: bool,
}

fn decode(public_key: &str) -> ApiResult<[u8; KEY_LEN]> {
    URL_SAFE_NO_PAD
        .decode(public_key)
        .ok()
        .and_then(|bytes| <[u8; KEY_LEN]>::try_from(bytes).ok())
        // The all-zero point is the identity: no secret can be derived with it.
        .filter(|key| key.iter().any(|b| *b != 0))
        .ok_or_else(|| ApiError::bad_request("invalid_key", "clave pública inválida"))
}

/// `PUT /me/e2e-key` -- publishes this device's public key, replacing any
/// previous one.
pub async fn publish(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<PublishKey>,
) -> ApiResult<Json<PublicKeyView>> {
    let key = decode(&req.public_key)?;
    rate_limit(&state, &format!("e2ekey:{}", auth.user_id), 10, 3600).await?;
    let previous: Option<Vec<u8>> = sqlx::query_scalar("SELECT public_key FROM user_keys WHERE user_id = $1")
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await?;
    let changed = previous.as_deref().is_some_and(|old| old != key);
    let updated_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO user_keys (user_id, public_key) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET
             public_key = EXCLUDED.public_key,
             updated_at = CASE WHEN user_keys.public_key = EXCLUDED.public_key
                               THEN user_keys.updated_at ELSE now() END
         RETURNING updated_at",
    )
    .bind(auth.user_id)
    .bind(key.as_slice())
    .fetch_one(&state.db)
    .await?;
    Ok(Json(PublicKeyView {
        public_key: URL_SAFE_NO_PAD.encode(key),
        updated_at,
        changed,
    }))
}

/// `GET /users/:id/e2e-key` -- a friend's (or one's own) public key.
pub async fn get(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PublicKeyView>> {
    if id != auth.user_id && !are_friends(&state, auth.user_id, id).await? {
        return Err(ApiError::forbidden("solo puedes ver las claves de tus amigos"));
    }
    let row: Option<(Vec<u8>, DateTime<Utc>)> =
        sqlx::query_as("SELECT public_key, updated_at FROM user_keys WHERE user_id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let (public_key, updated_at) =
        row.ok_or_else(|| ApiError::not_found("ese usuario aún no tiene cifrado de extremo a extremo"))?;
    Ok(Json(PublicKeyView {
        public_key: URL_SAFE_NO_PAD.encode(public_key),
        updated_at,
        changed: false,
    }))
}
