use crate::error::ProtocolError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Handshake sent once, immediately after opening a TCP connection to the
/// hub's tunnel port, before any raw game bytes flow. Length-prefixed by the
/// caller (see each side's `tcp_tunnel_*` module) since a bare TCP stream has
/// no message boundaries of its own; everything after this one frame is an
/// opaque byte pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TunnelHello {
    /// The room's host opens exactly one of these and keeps it open for the
    /// room's lifetime. The hub uses it purely as a signaling channel: one
    /// `0x01` byte written to this connection means "open a `HostBackend`
    /// connection now, a joiner is waiting."
    HostControl { room_token: Uuid },
    /// Opened by the host in response to a signal on its `HostControl`
    /// connection. Paired with the oldest still-waiting `JoinerStream` for
    /// the same room; once paired, the hub just copies bytes both ways.
    HostBackend { room_token: Uuid },
    /// Opened by a joiner's local relay every time the joiner's own game
    /// client connects to it, wanting to reach the real host.
    JoinerStream { room_token: Uuid },
}

impl TunnelHello {
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
    fn tunnel_hello_roundtrips_through_encode_decode() {
        let hello = TunnelHello::JoinerStream {
            room_token: Uuid::new_v4(),
        };
        let encoded = hello.encode().expect("encode should succeed");
        let decoded = TunnelHello::decode(&encoded).expect("decode should succeed");
        match (hello, decoded) {
            (TunnelHello::JoinerStream { room_token: a }, TunnelHello::JoinerStream { room_token: b }) => {
                assert_eq!(a, b);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }
}
