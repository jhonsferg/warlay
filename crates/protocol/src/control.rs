use crate::error::ProtocolError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverRequest {
    pub client_id: String,
    pub protocol_version: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnounceResponse {
    pub server_name: String,
    pub protocol_version: u16,
    pub relay_port: u16,
    pub known_peers: u32,
    pub uptime_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservedGame {
    pub source_addr: String,
    pub game_name: Option<String>,
    pub last_seen_secs_ago: u64,
    pub payload_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListGamesRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GamesListResponse {
    pub games: Vec<ObservedGame>,
}

/// Control-plane messages exchanged on the bridge's dedicated control port.
/// Kept separate from the data-plane [`crate::RelayEnvelope`] traffic so that
/// discovery and telemetry never interfere with raw game datagrams.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlMessage {
    Discover(DiscoverRequest),
    Announce(AnnounceResponse),
    ListGames(ListGamesRequest),
    GamesList(GamesListResponse),
}

impl ControlMessage {
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        let mut buf = Vec::with_capacity(64);
        buf.extend_from_slice(&crate::PROTOCOL_MAGIC);
        let body = bincode::serialize(self).map_err(ProtocolError::Serialize)?;
        buf.extend_from_slice(&body);
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < crate::PROTOCOL_MAGIC.len() || bytes[..4] != crate::PROTOCOL_MAGIC {
            return Err(ProtocolError::InvalidMagic);
        }
        bincode::deserialize(&bytes[4..]).map_err(ProtocolError::Deserialize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_roundtrips_through_encode_decode() {
        let message = ControlMessage::Discover(DiscoverRequest {
            client_id: "launcher-1".to_string(),
            protocol_version: crate::PROTOCOL_VERSION,
        });

        let encoded = message.encode().expect("encode should succeed");
        let decoded = ControlMessage::decode(&encoded).expect("decode should succeed");

        match decoded {
            ControlMessage::Discover(request) => {
                assert_eq!(request.client_id, "launcher-1");
                assert_eq!(request.protocol_version, crate::PROTOCOL_VERSION);
            }
            other => panic!("unexpected variant decoded: {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_missing_magic_prefix() {
        let bytes = bincode::serialize(&ControlMessage::ListGames(ListGamesRequest)).unwrap();
        let error = ControlMessage::decode(&bytes).unwrap_err();
        assert!(matches!(error, ProtocolError::InvalidMagic));
    }

    #[test]
    fn decode_rejects_truncated_frame() {
        let error = ControlMessage::decode(&[0x57, 0x4c]).unwrap_err();
        assert!(matches!(error, ProtocolError::InvalidMagic));
    }
}
