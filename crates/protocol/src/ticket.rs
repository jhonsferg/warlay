use crate::error::ProtocolError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What the main server vouches for when it hands a player a room session:
/// this user is a member of this room (and whether they host it), until
/// `expires_unix`. Helpers only ever see it signed (`crypto::sign_token`
/// with the secret they share with the main server) and verify it locally,
/// so they need no database and no knowledge of accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomTicket {
    pub room_id: Uuid,
    pub user_id: Uuid,
    pub is_host: bool,
    pub expires_unix: u64,
}

impl RoomTicket {
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        bincode::serialize(self).map_err(ProtocolError::Serialize)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        bincode::deserialize(bytes).map_err(ProtocolError::Deserialize)
    }

    pub fn is_expired(&self, now_unix: u64) -> bool {
        now_unix >= self.expires_unix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_roundtrips_and_expires() {
        let ticket = RoomTicket {
            room_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            is_host: true,
            expires_unix: 100,
        };
        assert_eq!(RoomTicket::decode(&ticket.encode().unwrap()).unwrap(), ticket);
        assert!(!ticket.is_expired(99));
        assert!(ticket.is_expired(100));
    }
}
