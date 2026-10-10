use crate::error::ProtocolError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// First byte of every datagram exchanged with a helper's relay port, so
/// control messages and relayed data never get confused.
pub const RELAY_CONTROL_TAG: u8 = 0xC1;
pub const RELAY_DATA_TAG: u8 = 0xD1;
/// `RELAY_DATA_TAG` + 8-byte channel id.
pub const RELAY_DATA_HEADER_LEN: usize = 9;

/// Last-resort relay on a helper server (TURN-like), for peers that can't
/// hole-punch each other (symmetric NAT). The helper only forwards opaque
/// datagrams between the two members of a channel: the QUIC session on top
/// stays end-to-end encrypted between the players.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelayControl {
    /// Client -> helper: bind this socket as my end of the channel to
    /// `peer_id` in the ticket's room. Re-sent periodically as keep-alive.
    Bind { ticket: Vec<u8>, peer_id: Uuid },
    /// Helper -> client: the channel id to prefix data with, and whether
    /// the peer has bound its end yet.
    Bound { channel: u64, peer_ready: bool },
    /// Helper -> client: the bind was refused (bad/expired ticket).
    Refused { reason: String },
}

impl RelayControl {
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        let mut buf = vec![RELAY_CONTROL_TAG];
        buf.extend(bincode::serialize(self).map_err(ProtocolError::Serialize)?);
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        match bytes.split_first() {
            Some((&RELAY_CONTROL_TAG, rest)) => bincode::deserialize(rest).ok(),
            _ => None,
        }
    }
}

/// Prefixes `payload` with the data header for `channel`.
pub fn encode_relay_data(channel: u64, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(RELAY_DATA_HEADER_LEN + payload.len());
    buf.push(RELAY_DATA_TAG);
    buf.extend_from_slice(&channel.to_le_bytes());
    buf.extend_from_slice(payload);
    buf
}

/// Splits a relay data datagram into its channel id and payload.
pub fn decode_relay_data(bytes: &[u8]) -> Option<(u64, &[u8])> {
    if bytes.len() < RELAY_DATA_HEADER_LEN || bytes[0] != RELAY_DATA_TAG {
        return None;
    }
    let channel = u64::from_le_bytes(bytes[1..9].try_into().ok()?);
    Some((channel, &bytes[RELAY_DATA_HEADER_LEN..]))
}

/// What a helper reports to the main server on every heartbeat (JSON over
/// HTTPS, authenticated with the shared helper secret).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperHeartbeat {
    pub helper_id: Uuid,
    pub name: String,
    pub region: String,
    /// Public `host:port` players use for UDP rendezvous.
    pub rendezvous_addr: String,
    /// Public `host:port` of the last-resort UDP relay.
    pub relay_addr: String,
    /// Rooms currently registered on this helper, for load balancing.
    pub active_rooms: u32,
    pub relay_channels: u32,
    /// Public base URL of the helper's edge (realtime socket + chat), e.g.
    /// `https://helper1.example.com` -- empty for helpers without one.
    #[serde(default)]
    pub edge_url: String,
    /// Players currently connected to that edge.
    #[serde(default)]
    pub clients: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_and_data_frames_are_distinguishable() {
        let bind = RelayControl::Bind {
            ticket: vec![1, 2, 3],
            peer_id: Uuid::new_v4(),
        };
        let encoded = bind.encode().unwrap();
        assert_eq!(RelayControl::decode(&encoded), Some(bind));
        assert_eq!(decode_relay_data(&encoded), None);

        let data = encode_relay_data(7, b"quic");
        assert_eq!(decode_relay_data(&data), Some((7, &b"quic"[..])));
        assert_eq!(RelayControl::decode(&data), None);
    }
}
