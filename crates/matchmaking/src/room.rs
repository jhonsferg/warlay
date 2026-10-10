use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use warlay_protocol::{RoomStatus, RoomSummary};

pub type RoomId = Uuid;
pub type ClientId = Uuid;

/// A hosted game lobby. `id` doubles as the UDP relay token: once a client
/// knows a room's id (via `RoomCreated`/`JoinedRoom`), it tags its relay
/// traffic with it so the hub can route packets to the right room without a
/// separate binding step.
#[derive(Debug, Clone)]
pub struct Room {
    pub id: RoomId,
    pub name: String,
    pub host_client_id: ClientId,
    pub host_display_name: String,
    pub game_port: u16,
    pub status: RoomStatus,
    pub player_count: u32,
    pub created_unix_secs: u64,
    /// `Some(unix_secs)` from the moment `player_count` last dropped to
    /// zero — cleared the moment anyone (re)joins. Drives
    /// `MatchmakingHub::purge_inactive_rooms`; never exposed in
    /// `RoomSummary`, it's purely server-side bookkeeping.
    pub empty_since: Option<u64>,
    /// Compared directly (this whole control channel is already encrypted
    /// end to end once the room lobby is reachable at all — see
    /// `lan-bridge-crypto` — so there's no separate hashing need for a
    /// lightweight per-room join gate that's never persisted to disk).
    password: Option<String>,
    pub members: HashSet<ClientId>,
}

impl Room {
    pub fn new(
        name: String,
        host_client_id: ClientId,
        host_display_name: String,
        game_port: u16,
        password: Option<String>,
    ) -> Self {
        let created_unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();

        Self {
            id: Uuid::new_v4(),
            name,
            host_client_id,
            host_display_name,
            game_port,
            status: RoomStatus::Available,
            player_count: 1,
            created_unix_secs,
            empty_since: None,
            password: password.filter(|value| !value.is_empty()),
            members: HashSet::new(),
        }
    }

    pub fn has_password(&self) -> bool {
        self.password.is_some()
    }

    pub fn password_matches(&self, attempt: Option<&str>) -> bool {
        match &self.password {
            None => true,
            Some(expected) => {
                attempt.is_some_and(|value| constant_time_eq::constant_time_eq(expected.as_bytes(), value.as_bytes()))
            }
        }
    }

    pub fn to_summary(&self) -> RoomSummary {
        RoomSummary {
            id: self.id,
            name: self.name.clone(),
            host_display_name: self.host_display_name.clone(),
            host_client_id: self.host_client_id,
            game_port: self.game_port,
            status: self.status.clone(),
            player_count: self.player_count,
            created_unix_secs: self.created_unix_secs,
            has_password: self.has_password(),
        }
    }
}
