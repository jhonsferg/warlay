use crate::error::ProtocolError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Hub mode's UDP data-plane frame. Unlike LAN-mesh mode's `RelayEnvelope`
/// (which routes by static peer address), the centralized hub has one shared
/// relay socket serving every room at once, so each datagram must carry the
/// room it belongs to. `room_token` is the room's id: unguessable (UUID v4)
/// and handed to a client only once it has authenticated and created or
/// joined that room over the WebSocket control plane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayFrame {
    pub room_token: Uuid,
    pub payload: Vec<u8>,
}

impl RelayFrame {
    pub fn new(room_token: Uuid, payload: Vec<u8>) -> Self {
        Self { room_token, payload }
    }

    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        bincode::serialize(self).map_err(ProtocolError::Serialize)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        bincode::deserialize(bytes).map_err(ProtocolError::Deserialize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_frame_roundtrips_through_encode_decode() {
        let frame = RelayFrame::new(Uuid::new_v4(), vec![0xF7, 0x2F, 0x04, 0x00]);
        let encoded = frame.encode().expect("encode should succeed");
        let decoded = RelayFrame::decode(&encoded).expect("decode should succeed");

        assert_eq!(decoded.room_token, frame.room_token);
        assert_eq!(decoded.payload, frame.payload);
    }
}
