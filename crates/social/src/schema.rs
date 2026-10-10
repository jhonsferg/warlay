//! The database schema, versioned in code: each migration runs once, in
//! order, inside a transaction, while holding a PostgreSQL advisory lock so
//! several main servers starting together don't race. Statements are
//! idempotent (`IF NOT EXISTS`), so a database created by an earlier build
//! is adopted as-is.

use sqlx::PgPool;
use tracing::info;

/// `(version, name, sql)`, in the order they must run. Never edit an
/// applied migration: add a new one.
const MIGRATIONS: &[(i64, &str, &str)] = &[(1, "initial", INITIAL)];

/// Arbitrary constant identifying Warlay's migration lock.
const LOCK_KEY: i64 = 0x5741_524C_4159;

pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(LOCK_KEY)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version     BIGINT PRIMARY KEY,
            name        TEXT NOT NULL,
            applied_at  TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .execute(&mut *tx)
    .await?;
    let applied: Vec<i64> = sqlx::query_scalar("SELECT version FROM schema_migrations")
        .fetch_all(&mut *tx)
        .await?;
    for (version, name, sql) in MIGRATIONS {
        if applied.contains(version) {
            continue;
        }
        sqlx::raw_sql(sql).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO schema_migrations (version, name) VALUES ($1, $2)")
            .bind(version)
            .bind(name)
            .execute(&mut *tx)
            .await?;
        info!(version, name, "applied database migration");
    }
    tx.commit().await
}

const INITIAL: &str = r#"
-- Warlay main server: accounts and social graph.

CREATE EXTENSION IF NOT EXISTS citext;

CREATE TABLE IF NOT EXISTS users (
    id              UUID PRIMARY KEY,
    username        CITEXT NOT NULL UNIQUE,
    email           CITEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    avatar_color    TEXT NOT NULL,
    bio             TEXT NOT NULL DEFAULT '',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at    TIMESTAMPTZ
);

-- One row per login; the refresh token is only ever stored hashed.
CREATE TABLE IF NOT EXISTS sessions (
    id                  UUID PRIMARY KEY,
    user_id             UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    refresh_token_hash  BYTEA NOT NULL UNIQUE,
    user_agent          TEXT NOT NULL DEFAULT '',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at          TIMESTAMPTZ NOT NULL,
    revoked_at          TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS sessions_user_idx ON sessions(user_id);

-- Directed while pending (requester -> addressee), symmetric once accepted.
CREATE TABLE IF NOT EXISTS friend_requests (
    id              UUID PRIMARY KEY,
    requester_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    addressee_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (requester_id, addressee_id),
    CHECK (requester_id <> addressee_id)
);
CREATE INDEX IF NOT EXISTS friend_requests_addressee_idx ON friend_requests(addressee_id);

-- Stored once per pair with user_a < user_b.
CREATE TABLE IF NOT EXISTS friendships (
    user_a      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_a, user_b),
    CHECK (user_a < user_b)
);
CREATE INDEX IF NOT EXISTS friendships_b_idx ON friendships(user_b);

CREATE TABLE IF NOT EXISTS communities (
    id          UUID PRIMARY KEY,
    name        TEXT NOT NULL,
    slug        CITEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    accent_color TEXT NOT NULL,
    is_public   BOOLEAN NOT NULL DEFAULT true,
    owner_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS community_members (
    community_id    UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role            TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member')),
    joined_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (community_id, user_id)
);
CREATE INDEX IF NOT EXISTS community_members_user_idx ON community_members(user_id);

-- Game rooms: the social record of a lobby. The game traffic itself never
-- touches this server; `helper_id` says which helper does rendezvous.
CREATE TABLE IF NOT EXISTS rooms (
    id              UUID PRIMARY KEY,
    name            TEXT NOT NULL,
    community_id    UUID REFERENCES communities(id) ON DELETE CASCADE,
    host_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    password_hash   TEXT,
    max_players     INT NOT NULL CHECK (max_players BETWEEN 2 AND 24),
    game_port       INT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('open', 'in_game', 'closed')),
    helper_id       UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at       TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS rooms_open_idx ON rooms(status) WHERE status <> 'closed';

CREATE TABLE IF NOT EXISTS room_members (
    room_id     UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (room_id, user_id)
);
-- A player is in at most one room at a time.
CREATE UNIQUE INDEX IF NOT EXISTS room_members_one_room_idx ON room_members(user_id);

-- Invitations to a community or a room.
CREATE TABLE IF NOT EXISTS invites (
    id              UUID PRIMARY KEY,
    kind            TEXT NOT NULL CHECK (kind IN ('community', 'room')),
    target_id       UUID NOT NULL,
    inviter_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    invitee_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status          TEXT NOT NULL CHECK (status IN ('pending', 'accepted', 'declined')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS invites_invitee_idx ON invites(invitee_id) WHERE status = 'pending';

-- Chat: `channel` is 'dm:<lowId>:<highId>', 'room:<id>' or 'community:<id>'.
CREATE TABLE IF NOT EXISTS messages (
    id          UUID PRIMARY KEY,
    channel     TEXT NOT NULL,
    sender_id   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS messages_channel_idx ON messages(channel, created_at DESC);

CREATE TABLE IF NOT EXISTS notifications (
    id          UUID PRIMARY KEY,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,
    payload     JSONB NOT NULL,
    read_at     TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS notifications_user_idx ON notifications(user_id, created_at DESC);
"#;
