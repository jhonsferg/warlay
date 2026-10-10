use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};
use tracing::{debug, info, warn};
use uuid::Uuid;
use warlay_protocol::{decode_relay_data, RelayControl, RELAY_CONTROL_TAG, RELAY_DATA_TAG};

use crate::udp_rendezvous::verify_ticket;

/// One player's end of a relay channel.
#[derive(Debug, Clone, Copy)]
struct End {
    user: Uuid,
    addr: SocketAddr,
}

#[derive(Debug)]
struct Channel {
    ends: [Option<End>; 2],
    last_activity: Instant,
}

impl Channel {
    fn other_end(&self, from: SocketAddr) -> Option<SocketAddr> {
        match (self.ends[0], self.ends[1]) {
            (Some(a), Some(b)) if a.addr == from => Some(b.addr),
            (Some(a), Some(b)) if b.addr == from => Some(a.addr),
            _ => None,
        }
    }
}

#[derive(Default)]
struct RelayState {
    channels: HashMap<u64, Channel>,
    /// (room, lower user id, higher user id) -> channel id.
    by_pair: HashMap<(Uuid, Uuid, Uuid), u64>,
}

/// Last-resort UDP relay run by helper servers: when two players can't
/// hole-punch each other, both bind a channel here (authorized by their
/// room tickets) and the helper forwards opaque datagrams between them. It
/// never sees game data in the clear -- the QUIC session runs end to end.
pub struct UdpRelay {
    socket: Arc<UdpSocket>,
    ticket_secret: Vec<u8>,
    state: Mutex<RelayState>,
    idle_ttl: Duration,
}

pub struct UdpRelayHandle {
    pub task: JoinHandle<()>,
}

impl UdpRelay {
    pub fn new(socket: UdpSocket, ticket_secret: impl Into<Vec<u8>>) -> Self {
        Self {
            socket: Arc::new(socket),
            ticket_secret: ticket_secret.into(),
            state: Mutex::new(RelayState::default()),
            idle_ttl: Duration::from_secs(60),
        }
    }

    pub fn spawn(self: Arc<Self>) -> UdpRelayHandle {
        UdpRelayHandle {
            task: tokio::spawn(self.run()),
        }
    }

    /// Channels with both ends bound, for load reporting.
    pub fn active_channels(&self) -> usize {
        let state = self.state.lock().unwrap();
        state
            .channels
            .values()
            .filter(|c| c.ends.iter().all(Option::is_some))
            .count()
    }

    async fn run(self: Arc<Self>) {
        let mut buf = vec![0_u8; 65_536];
        let mut cleanup = tokio::time::interval(Duration::from_secs(10));
        cleanup.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                res = self.socket.recv_from(&mut buf) => match res {
                    Ok((len, from)) => self.handle(&buf[..len], from).await,
                    Err(error) => warn!(%error, "udp relay receive error"),
                },
                _ = cleanup.tick() => self.cleanup(),
            }
        }
    }

    async fn handle(&self, datagram: &[u8], from: SocketAddr) {
        match datagram.first() {
            Some(&RELAY_DATA_TAG) => {
                let Some((channel, _)) = decode_relay_data(datagram) else {
                    return;
                };
                let target = {
                    let mut state = self.state.lock().unwrap();
                    let Some(ch) = state.channels.get_mut(&channel) else {
                        return;
                    };
                    let target = ch.other_end(from);
                    if target.is_some() {
                        ch.last_activity = Instant::now();
                    }
                    target
                };
                if let Some(target) = target {
                    let _ = self.socket.send_to(datagram, target).await;
                }
            }
            Some(&RELAY_CONTROL_TAG) => {
                if let Some(RelayControl::Bind { ticket, peer_id }) = RelayControl::decode(datagram) {
                    self.handle_bind(&ticket, peer_id, from).await;
                }
            }
            _ => {}
        }
    }

    async fn handle_bind(&self, ticket: &[u8], peer_id: Uuid, from: SocketAddr) {
        let Some(ticket) = verify_ticket(&self.ticket_secret, ticket) else {
            debug!(%from, "relay bind with an invalid or expired ticket");
            let refused = RelayControl::Refused {
                reason: "invalid ticket".into(),
            };
            if let Ok(bytes) = refused.encode() {
                let _ = self.socket.send_to(&bytes, from).await;
            }
            return;
        };
        if ticket.user_id == peer_id {
            return;
        }

        let (channel, peer_addr, newly_bound) = {
            let mut state = self.state.lock().unwrap();
            let key = (ticket.room_id, ticket.user_id.min(peer_id), ticket.user_id.max(peer_id));
            let channel = match state.by_pair.get(&key) {
                Some(id) => *id,
                None => {
                    let id = Uuid::new_v4().as_u64_pair().0;
                    state.by_pair.insert(key, id);
                    state.channels.insert(
                        id,
                        Channel {
                            ends: [None, None],
                            last_activity: Instant::now(),
                        },
                    );
                    id
                }
            };
            let ch = state.channels.get_mut(&channel).expect("channel indexed by pair");
            let slot = usize::from(ticket.user_id != key.1);
            let newly_bound = ch.ends[slot].is_none_or(|e| e.addr != from);
            ch.ends[slot] = Some(End {
                user: ticket.user_id,
                addr: from,
            });
            ch.last_activity = Instant::now();
            let peer_addr = ch.ends[1 - slot].filter(|e| e.user == peer_id).map(|e| e.addr);
            (channel, peer_addr, newly_bound)
        };

        if newly_bound {
            info!(room_id = %ticket.room_id, user_id = %ticket.user_id, %peer_id, %from, channel, "relay channel end bound");
        }
        let reply = RelayControl::Bound {
            channel,
            peer_ready: peer_addr.is_some(),
        };
        if let Ok(bytes) = reply.encode() {
            let _ = self.socket.send_to(&bytes, from).await;
        }
        // Let the end that was already waiting know its peer just arrived.
        if let (Some(peer_addr), true) = (peer_addr, newly_bound) {
            let notify = RelayControl::Bound {
                channel,
                peer_ready: true,
            };
            if let Ok(bytes) = notify.encode() {
                let _ = self.socket.send_to(&bytes, peer_addr).await;
            }
        }
    }

    fn cleanup(&self) {
        let mut state = self.state.lock().unwrap();
        let ttl = self.idle_ttl;
        let expired: Vec<u64> = state
            .channels
            .iter()
            .filter(|(_, c)| c.last_activity.elapsed() >= ttl)
            .map(|(id, _)| *id)
            .collect();
        for id in &expired {
            state.channels.remove(id);
        }
        state.by_pair.retain(|_, id| !expired.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use warlay_protocol::{encode_relay_data, RoomTicket};

    fn ticket(secret: &[u8], room_id: Uuid, user_id: Uuid) -> Vec<u8> {
        let t = RoomTicket {
            room_id,
            user_id,
            is_host: false,
            expires_unix: u64::MAX,
        };
        warlay_crypto::sign_token(secret, &t.encode().unwrap())
    }

    async fn recv_control(socket: &UdpSocket) -> RelayControl {
        let mut buf = [0_u8; 2048];
        let (n, _) = tokio::time::timeout(Duration::from_secs(2), socket.recv_from(&mut buf))
            .await
            .expect("timed out")
            .unwrap();
        RelayControl::decode(&buf[..n]).expect("control frame")
    }

    #[tokio::test]
    async fn forwards_between_the_two_bound_ends_only() {
        let secret = b"helper-secret".to_vec();
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server.local_addr().unwrap();
        Arc::new(UdpRelay::new(server, secret.clone())).spawn();

        let room = Uuid::new_v4();
        let (a_id, b_id) = (Uuid::new_v4(), Uuid::new_v4());
        let a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let b = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        let bind_a = RelayControl::Bind {
            ticket: ticket(&secret, room, a_id),
            peer_id: b_id,
        };
        a.send_to(&bind_a.encode().unwrap(), server_addr).await.unwrap();
        let RelayControl::Bound { channel, peer_ready } = recv_control(&a).await else {
            panic!("expected Bound");
        };
        assert!(!peer_ready);

        let bind_b = RelayControl::Bind {
            ticket: ticket(&secret, room, b_id),
            peer_id: a_id,
        };
        b.send_to(&bind_b.encode().unwrap(), server_addr).await.unwrap();
        assert_eq!(
            recv_control(&b).await,
            RelayControl::Bound {
                channel,
                peer_ready: true
            }
        );
        assert_eq!(
            recv_control(&a).await,
            RelayControl::Bound {
                channel,
                peer_ready: true
            }
        );

        a.send_to(&encode_relay_data(channel, b"hello"), server_addr)
            .await
            .unwrap();
        let mut buf = [0_u8; 64];
        let (n, _) = tokio::time::timeout(Duration::from_secs(2), b.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(decode_relay_data(&buf[..n]), Some((channel, &b"hello"[..])));

        // A stranger knowing the channel id gets nothing through.
        let stranger = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        stranger
            .send_to(&encode_relay_data(channel, b"spam"), server_addr)
            .await
            .unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(200), b.recv_from(&mut buf))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn refuses_forged_tickets() {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server.local_addr().unwrap();
        Arc::new(UdpRelay::new(server, b"real".to_vec())).spawn();

        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let bind = RelayControl::Bind {
            ticket: ticket(b"forged", Uuid::new_v4(), Uuid::new_v4()),
            peer_id: Uuid::new_v4(),
        };
        client.send_to(&bind.encode().unwrap(), server_addr).await.unwrap();
        assert!(matches!(recv_control(&client).await, RelayControl::Refused { .. }));
    }
}
