use crate::error::ProtocolError;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

/// Information about a participant in a room rendezvous session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeerInfo {
    pub client_id: Uuid,
    pub addr: SocketAddr,
    pub is_host: bool,
}

/// UDP signaling (rendezvous) messages sent between peers and the hub
/// for N-participant room presence and direct P2P endpoint exchange.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RendezvousMessage {
    /// Sent by a peer to register presence in a room.
    Register {
        room_token: Uuid,
        client_id: Uuid,
        is_host: bool,
    },
    /// Sent periodically by a peer to refresh its room registration.
    Heartbeat { room_token: Uuid, client_id: Uuid },
    /// Sent by the hub to a newly registered peer containing all existing peers.
    PeerList { peers: Vec<PeerInfo> },
    /// Broadcast by the hub to existing peers when a new peer registers.
    PeerJoined { peer: PeerInfo },
    /// Broadcast by the hub when a peer explicitly leaves or times out.
    PeerLeft { client_id: Uuid },
    /// Sent by a peer from the UDP socket it is about to hole-punch with,
    /// to learn the public address the hub sees that socket as (STUN-like).
    /// Behind a NAT that rewrites ports, the socket's local port says
    /// nothing about where peers must send to reach it.
    Probe,
    /// The hub's answer to `Probe`: the source address the probe came from.
    ProbeReply { observed: SocketAddr },
    /// Sent by a peer for the hub to forward verbatim to another member of
    /// the same room, which receives this same message. Carries P2P link
    /// offers when the peers can't reach each other's registration socket
    /// directly (e.g. two players behind one router without hairpinning).
    Signal {
        room_token: Uuid,
        from: Uuid,
        to: Uuid,
        payload: Vec<u8>,
    },
    /// `Register` for helpers that only admit ticket holders: the room,
    /// identity and host flag all come from a `RoomTicket` signed by the
    /// main server instead of being taken on the sender's word.
    RegisterWithTicket { ticket: Vec<u8> },
}

impl RendezvousMessage {
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
    fn register_message_roundtrips_through_encode_decode() {
        let msg = RendezvousMessage::Register {
            room_token: Uuid::new_v4(),
            client_id: Uuid::new_v4(),
            is_host: true,
        };
        let encoded = msg.encode().expect("encode should succeed");
        let decoded = RendezvousMessage::decode(&encoded).expect("decode should succeed");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn heartbeat_message_roundtrips_through_encode_decode() {
        let msg = RendezvousMessage::Heartbeat {
            room_token: Uuid::new_v4(),
            client_id: Uuid::new_v4(),
        };
        let encoded = msg.encode().expect("encode should succeed");
        let decoded = RendezvousMessage::decode(&encoded).expect("decode should succeed");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn peer_list_message_roundtrips_through_encode_decode() {
        let addr1: SocketAddr = "192.168.1.100:12345".parse().expect("valid address");
        let addr2: SocketAddr = "10.0.0.5:54321".parse().expect("valid address");
        let peer1 = PeerInfo {
            client_id: Uuid::new_v4(),
            addr: addr1,
            is_host: true,
        };
        let peer2 = PeerInfo {
            client_id: Uuid::new_v4(),
            addr: addr2,
            is_host: false,
        };
        let msg = RendezvousMessage::PeerList {
            peers: vec![peer1, peer2],
        };
        let encoded = msg.encode().expect("encode should succeed");
        let decoded = RendezvousMessage::decode(&encoded).expect("decode should succeed");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn peer_joined_message_roundtrips_through_encode_decode() {
        let addr: SocketAddr = "192.168.1.101:23456".parse().expect("valid address");
        let peer = PeerInfo {
            client_id: Uuid::new_v4(),
            addr,
            is_host: false,
        };
        let msg = RendezvousMessage::PeerJoined { peer };
        let encoded = msg.encode().expect("encode should succeed");
        let decoded = RendezvousMessage::decode(&encoded).expect("decode should succeed");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn probe_messages_roundtrip_through_encode_decode() {
        let observed: SocketAddr = "203.0.113.7:50123".parse().expect("valid address");
        let signal = RendezvousMessage::Signal {
            room_token: Uuid::new_v4(),
            from: Uuid::new_v4(),
            to: Uuid::new_v4(),
            payload: vec![1, 2, 3],
        };
        for msg in [
            RendezvousMessage::Probe,
            RendezvousMessage::ProbeReply { observed },
            signal,
        ] {
            let encoded = msg.encode().expect("encode should succeed");
            let decoded = RendezvousMessage::decode(&encoded).expect("decode should succeed");
            assert_eq!(msg, decoded);
        }
    }

    #[test]
    fn peer_left_message_roundtrips_through_encode_decode() {
        let msg = RendezvousMessage::PeerLeft {
            client_id: Uuid::new_v4(),
        };
        let encoded = msg.encode().expect("encode should succeed");
        let decoded = RendezvousMessage::decode(&encoded).expect("decode should succeed");
        assert_eq!(msg, decoded);
    }
}
