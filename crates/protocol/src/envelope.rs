use crate::error::ProtocolError;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Tells a receiving peer how a relayed datagram should be re-injected into
/// its local subnet once it arrives through the peer tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PacketOrigin {
    Broadcast,
    Unicast,
}

/// Wraps a raw datagram captured on one bridge's local subnet so that a peer
/// bridge on a different subnet knows both the original sender and how the
/// packet should be reflected once it lands there.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayEnvelope {
    pub origin_addr: SocketAddr,
    pub origin_kind: PacketOrigin,
    pub payload: Vec<u8>,
}

impl RelayEnvelope {
    pub fn new(origin_addr: SocketAddr, origin_kind: PacketOrigin, payload: Vec<u8>) -> Self {
        Self {
            origin_addr,
            origin_kind,
            payload,
        }
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
    fn envelope_roundtrips_through_encode_decode() {
        let envelope = RelayEnvelope::new(
            "192.168.1.42:6112".parse().unwrap(),
            PacketOrigin::Broadcast,
            vec![0xF7, 0x2F, 0x04, 0x00],
        );

        let encoded = envelope.encode().expect("encode should succeed");
        let decoded = RelayEnvelope::decode(&encoded).expect("decode should succeed");

        assert_eq!(decoded.origin_addr, envelope.origin_addr);
        assert_eq!(decoded.origin_kind, envelope.origin_kind);
        assert_eq!(decoded.payload, envelope.payload);
    }
}
