use crate::room::{ClientId, Room, RoomId};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};
use warlay_protocol::RoomStatus;

/// In-memory registry of every active room on this server. The VPS is the
/// single source of truth for room state — there is no persistence, which is
/// correct here: rooms are ephemeral across server restarts. Within a
/// server's lifetime, a room outlives its host merely disconnecting — it's
/// removed only by an explicit `DeleteRoom` from its creator (see
/// `MatchmakingHub::handle_delete_room`), or by sitting with zero connected
/// participants for too long (see `MatchmakingHub::purge_inactive_rooms`).
#[derive(Default)]
pub struct RoomRegistry {
    rooms: RwLock<HashMap<RoomId, Room>>,
}

impl RoomRegistry {
    pub fn create(
        &self,
        name: String,
        host_client_id: ClientId,
        host_display_name: String,
        game_port: u16,
        password: Option<String>,
    ) -> Room {
        let room = Room::new(name, host_client_id, host_display_name, game_port, password);
        self.rooms
            .write()
            .expect("room registry lock poisoned")
            .insert(room.id, room.clone());
        room
    }

    pub fn get(&self, id: RoomId) -> Option<Room> {
        self.rooms
            .read()
            .expect("room registry lock poisoned")
            .get(&id)
            .cloned()
    }

    pub fn remove(&self, id: RoomId) -> Option<Room> {
        self.rooms.write().expect("room registry lock poisoned").remove(&id)
    }

    pub fn list(&self) -> Vec<Room> {
        self.rooms
            .read()
            .expect("room registry lock poisoned")
            .values()
            .cloned()
            .collect()
    }

    pub fn adjust_player_count(&self, id: RoomId, delta: i32) -> Option<Room> {
        let mut rooms = self.rooms.write().expect("room registry lock poisoned");
        let room = rooms.get_mut(&id)?;
        room.player_count = room.player_count.saturating_add_signed(delta);
        room.empty_since = if room.player_count == 0 { Some(unix_now()) } else { None };
        Some(room.clone())
    }

    pub fn set_status(&self, id: RoomId, status: RoomStatus) -> Option<Room> {
        let mut rooms = self.rooms.write().expect("room registry lock poisoned");
        let room = rooms.get_mut(&id)?;
        room.status = status;
        Some(room.clone())
    }

    pub fn add_member(&self, id: RoomId, client_id: ClientId) -> Option<Room> {
        let mut rooms = self.rooms.write().expect("room registry lock poisoned");
        let room = rooms.get_mut(&id)?;
        room.members.insert(client_id);
        Some(room.clone())
    }

    pub fn remove_member(&self, id: RoomId, client_id: ClientId) -> Option<Room> {
        let mut rooms = self.rooms.write().expect("room registry lock poisoned");
        let room = rooms.get_mut(&id)?;
        room.members.remove(&client_id);
        Some(room.clone())
    }

    /// Every room that's been sitting at zero connected participants since
    /// before `cutoff_unix_secs` — the caller (`MatchmakingHub`) is
    /// responsible for actually removing and announcing each one, since
    /// that also needs to broadcast `RoomRemoved`.
    pub fn empty_since_before(&self, cutoff_unix_secs: u64) -> Vec<RoomId> {
        self.rooms
            .read()
            .expect("room registry lock poisoned")
            .values()
            .filter_map(|room| match room.empty_since {
                Some(since) if since < cutoff_unix_secs => Some(room.id),
                _ => None,
            })
            .collect()
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_then_list_contains_the_room() {
        let registry = RoomRegistry::default();
        let room = registry.create("Sala 1".to_string(), ClientId::new_v4(), "Host".to_string(), 6112, None);

        let listed = registry.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, room.id);
        assert_eq!(listed[0].player_count, 1);
    }

    #[test]
    fn add_and_remove_member_tracks_membership() {
        let registry = RoomRegistry::default();
        let room = registry.create("Sala 1".to_string(), ClientId::new_v4(), "Host".to_string(), 6112, None);
        let client_a = ClientId::new_v4();
        let client_b = ClientId::new_v4();

        let updated = registry.add_member(room.id, client_a).unwrap();
        assert!(updated.members.contains(&client_a));

        let updated2 = registry.add_member(room.id, client_b).unwrap();
        assert!(updated2.members.contains(&client_a));
        assert!(updated2.members.contains(&client_b));

        let removed = registry.remove_member(room.id, client_a).unwrap();
        assert!(!removed.members.contains(&client_a));
        assert!(removed.members.contains(&client_b));
    }

    #[test]
    fn adjust_player_count_saturates_and_persists() {
        let registry = RoomRegistry::default();
        let room = registry.create("Sala 1".to_string(), ClientId::new_v4(), "Host".to_string(), 6112, None);

        let updated = registry.adjust_player_count(room.id, 2).unwrap();
        assert_eq!(updated.player_count, 3);

        let decremented = registry.adjust_player_count(room.id, -10).unwrap();
        assert_eq!(decremented.player_count, 0);
    }

    #[test]
    fn dropping_to_zero_players_marks_the_room_empty_and_refilling_clears_it() {
        let registry = RoomRegistry::default();
        let room = registry.create("Sala 1".to_string(), ClientId::new_v4(), "Host".to_string(), 6112, None);
        assert!(
            room.empty_since.is_none(),
            "a freshly created room has its host connected"
        );

        let emptied = registry.adjust_player_count(room.id, -1).unwrap();
        assert_eq!(emptied.player_count, 0);
        assert!(emptied.empty_since.is_some());

        let refilled = registry.adjust_player_count(room.id, 1).unwrap();
        assert!(
            refilled.empty_since.is_none(),
            "someone joining should clear the empty marker"
        );
    }

    #[test]
    fn empty_since_before_only_matches_rooms_emptied_before_the_cutoff() {
        let registry = RoomRegistry::default();
        let stale = registry.create("Vieja".to_string(), ClientId::new_v4(), "Host".to_string(), 6112, None);
        let fresh = registry.create("Nueva".to_string(), ClientId::new_v4(), "Host".to_string(), 6112, None);
        registry.adjust_player_count(stale.id, -1).unwrap();
        registry.adjust_player_count(fresh.id, -1).unwrap();

        // A cutoff far in the future catches anything already empty...
        let far_future_cutoff = unix_now() + 10_000;
        let matches = registry.empty_since_before(far_future_cutoff);
        assert!(matches.contains(&stale.id));
        assert!(matches.contains(&fresh.id));

        // ...while a cutoff in the past catches nothing yet.
        assert!(registry.empty_since_before(0).is_empty());
    }

    #[test]
    fn remove_drops_the_room() {
        let registry = RoomRegistry::default();
        let room = registry.create("Sala 1".to_string(), ClientId::new_v4(), "Host".to_string(), 6112, None);

        assert!(registry.remove(room.id).is_some());
        assert!(registry.list().is_empty());
    }

    #[test]
    fn password_protected_room_rejects_wrong_or_missing_password() {
        let registry = RoomRegistry::default();
        let room = registry.create(
            "Sala privada".to_string(),
            ClientId::new_v4(),
            "Host".to_string(),
            6112,
            Some("secreta".to_string()),
        );

        let stored = registry.get(room.id).unwrap();
        assert!(stored.has_password());
        assert!(!stored.password_matches(None));
        assert!(!stored.password_matches(Some("incorrecta")));
        assert!(stored.password_matches(Some("secreta")));
    }
}
