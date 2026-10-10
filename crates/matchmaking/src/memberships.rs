use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Tracks which UDP endpoints (as observed by the hub — i.e. post-NAT public
/// addresses) currently belong to each room. A client is "in" a room's
/// membership set the moment its relay uplink sends its first `RelayFrame`
/// tagged with that room's token; membership refreshes on every subsequent
/// frame and expires if no frame arrives within the TTL, acting as a
/// self-healing NAT keep-alive table without any explicit leave handshake on
/// the UDP side.
#[derive(Default)]
pub struct RoomMemberships {
    rooms: RwLock<HashMap<Uuid, HashMap<SocketAddr, Instant>>>,
}

impl RoomMemberships {
    pub fn touch(&self, room_token: Uuid, addr: SocketAddr) {
        let mut rooms = self.rooms.write().expect("room memberships lock poisoned");
        rooms.entry(room_token).or_default().insert(addr, Instant::now());
    }

    pub fn members_excluding(&self, room_token: Uuid, exclude: SocketAddr) -> Vec<SocketAddr> {
        let rooms = self.rooms.read().expect("room memberships lock poisoned");
        rooms
            .get(&room_token)
            .map(|members| members.keys().filter(|addr| **addr != exclude).copied().collect())
            .unwrap_or_default()
    }

    pub fn drop_room(&self, room_token: Uuid) {
        self.rooms
            .write()
            .expect("room memberships lock poisoned")
            .remove(&room_token);
    }

    pub fn prune_expired(&self, ttl: Duration) {
        let now = Instant::now();
        let mut rooms = self.rooms.write().expect("room memberships lock poisoned");
        rooms.retain(|_, members| {
            members.retain(|_, last_seen| now.duration_since(*last_seen) < ttl);
            !members.is_empty()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};
    use std::thread::sleep;

    fn addr(port: u16) -> SocketAddr {
        SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
    }

    #[test]
    fn touch_then_members_excluding_omits_the_sender() {
        let memberships = RoomMemberships::default();
        let token = Uuid::new_v4();
        memberships.touch(token, addr(1000));
        memberships.touch(token, addr(2000));

        let targets = memberships.members_excluding(token, addr(1000));
        assert_eq!(targets, vec![addr(2000)]);
    }

    #[test]
    fn prune_expired_removes_stale_members_and_empty_rooms() {
        let memberships = RoomMemberships::default();
        let token = Uuid::new_v4();
        memberships.touch(token, addr(1000));

        sleep(Duration::from_millis(30));
        memberships.prune_expired(Duration::from_millis(10));

        assert!(memberships.members_excluding(token, addr(9999)).is_empty());
    }

    #[test]
    fn drop_room_clears_all_members() {
        let memberships = RoomMemberships::default();
        let token = Uuid::new_v4();
        memberships.touch(token, addr(1000));
        memberships.drop_room(token);

        assert!(memberships.members_excluding(token, addr(9999)).is_empty());
    }
}
