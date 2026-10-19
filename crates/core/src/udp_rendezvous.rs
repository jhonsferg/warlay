use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};
use tracing::{debug, info, warn};
use uuid::Uuid;
use warlay_protocol::{PeerInfo, RendezvousMessage, RoomTicket};

#[derive(Debug)]
struct PeerState {
    addr: SocketAddr,
    is_host: bool,
    last_seen: Instant,
}

/// UDP rendezvous server responsible for N-participant room presence and direct P2P endpoint exchange.
///
/// Participants send `RendezvousMessage::Register` with `room_token`, `client_id`, and `is_host`.
/// The hub responds to the registrant with `PeerList` (all existing peers in that room)
/// and broadcasts `PeerJoined` to all other existing peers in that room.
///
/// Participants periodically send `RendezvousMessage::Heartbeat`, updating `last_seen`.
/// If a participant fails to send heartbeats within `ttl`, they are purged from the room
/// and `PeerLeft` is broadcast to remaining peers.
pub struct UdpRendezvous {
    socket: Arc<UdpSocket>,
    rooms: Arc<Mutex<HashMap<Uuid, HashMap<Uuid, PeerState>>>>,
    ttl: Duration,
    /// When set, only `RegisterWithTicket` carrying a ticket signed with this
    /// secret is admitted (helper servers); plain `Register` is ignored.
    ticket_secret: Option<Vec<u8>>,
}

pub struct UdpRendezvousHandle {
    pub task: JoinHandle<()>,
}

/// Checks a signed `RoomTicket` and that it hasn't expired.
pub fn verify_ticket(secret: &[u8], token: &[u8]) -> Option<RoomTicket> {
    let payload = warlay_crypto::verify_token(secret, token)?;
    let ticket = RoomTicket::decode(payload).ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    (!ticket.is_expired(now)).then_some(ticket)
}

impl UdpRendezvous {
    pub fn new(socket: UdpSocket) -> Self {
        Self::with_ttl(socket, Duration::from_secs(60))
    }

    pub fn with_ttl(socket: UdpSocket, ttl: Duration) -> Self {
        Self {
            socket: Arc::new(socket),
            rooms: Arc::new(Mutex::new(HashMap::new())),
            ttl,
            ticket_secret: None,
        }
    }

    /// Admits only players holding a `RoomTicket` signed with `secret`.
    pub fn with_ticket_secret(mut self, secret: impl Into<Vec<u8>>) -> Self {
        self.ticket_secret = Some(secret.into());
        self
    }

    /// Rooms with at least one registered player, for load reporting.
    pub async fn active_rooms(&self) -> usize {
        self.rooms.lock().await.len()
    }

    pub fn spawn(self: Arc<Self>) -> UdpRendezvousHandle {
        let task = tokio::spawn(self.run());
        UdpRendezvousHandle { task }
    }

    async fn run(self: Arc<Self>) {
        let mut buf = [0_u8; 2048];
        let cleanup_interval_duration = std::cmp::max(Duration::from_millis(50), self.ttl / 3);
        let mut cleanup_interval = tokio::time::interval(cleanup_interval_duration);
        cleanup_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                recv_res = self.socket.recv_from(&mut buf) => {
                    match recv_res {
                        Ok((len, peer_addr)) => {
                            self.handle_datagram(&buf[..len], peer_addr).await;
                        }
                        Err(error) => {
                            warn!(%error, "udp rendezvous receive error");
                        }
                    }
                }
                _ = cleanup_interval.tick() => {
                    self.cleanup_expired().await;
                }
            }
        }
    }

    async fn handle_datagram(&self, data: &[u8], peer_addr: SocketAddr) {
        let msg = match RendezvousMessage::decode(data) {
            Ok(msg) => msg,
            Err(error) => {
                debug!(%error, %peer_addr, "failed to decode rendezvous message");
                return;
            }
        };

        match msg {
            RendezvousMessage::Register {
                room_token,
                client_id,
                is_host,
            } => {
                if self.ticket_secret.is_some() {
                    debug!(%peer_addr, "ignoring ticketless registration");
                    return;
                }
                self.handle_register(room_token, client_id, is_host, peer_addr).await;
            }
            RendezvousMessage::RegisterWithTicket { ticket } => {
                let Some(secret) = &self.ticket_secret else {
                    debug!(%peer_addr, "ticket registration on an open rendezvous, ignoring");
                    return;
                };
                match verify_ticket(secret, &ticket) {
                    Some(t) => self.handle_register(t.room_id, t.user_id, t.is_host, peer_addr).await,
                    None => debug!(%peer_addr, "rejected registration with an invalid or expired ticket"),
                }
            }
            RendezvousMessage::Heartbeat { room_token, client_id } => {
                self.handle_heartbeat(room_token, client_id, peer_addr).await;
            }
            RendezvousMessage::Signal {
                room_token, from, to, ..
            } => {
                self.handle_signal(room_token, from, to, data, peer_addr).await;
            }
            RendezvousMessage::Probe => {
                let reply = RendezvousMessage::ProbeReply { observed: peer_addr };
                if let Ok(bytes) = reply.encode() {
                    if let Err(error) = self.socket.send_to(&bytes, peer_addr).await {
                        debug!(%error, %peer_addr, "failed to send ProbeReply");
                    }
                }
            }
            RendezvousMessage::PeerList { .. }
            | RendezvousMessage::PeerJoined { .. }
            | RendezvousMessage::PeerLeft { .. }
            | RendezvousMessage::ProbeReply { .. }
            | RendezvousMessage::RegisterAgain => {
                debug!(%peer_addr, "received client-bound message on server, ignoring");
            }
        }
    }

    async fn handle_register(&self, room_token: Uuid, client_id: Uuid, is_host: bool, peer_addr: SocketAddr) {
        info!(%room_token, %client_id, is_host, %peer_addr, "received rendezvous registration");

        let (existing_peers_list, existing_peer_addrs) = {
            let mut rooms = self.rooms.lock().await;
            let room = rooms.entry(room_token).or_default();

            // Collect info of existing peers in the room before/excluding this client
            let existing_peers_list: Vec<PeerInfo> = room
                .iter()
                .filter(|(&id, _)| id != client_id)
                .map(|(&id, state)| PeerInfo {
                    client_id: id,
                    addr: state.addr,
                    is_host: state.is_host,
                })
                .collect();

            let existing_peer_addrs: Vec<SocketAddr> = existing_peers_list.iter().map(|p| p.addr).collect();

            // Insert or update registering peer
            room.insert(
                client_id,
                PeerState {
                    addr: peer_addr,
                    is_host,
                    last_seen: Instant::now(),
                },
            );

            (existing_peers_list, existing_peer_addrs)
        };

        // Send PeerList to the registering participant (containing all existing peers)
        let peer_list_msg = RendezvousMessage::PeerList {
            peers: existing_peers_list,
        };
        if let Ok(bytes) = peer_list_msg.encode() {
            if let Err(error) = self.socket.send_to(&bytes, peer_addr).await {
                warn!(%error, %room_token, %client_id, %peer_addr, "failed to send PeerList to registrant");
            }
        }

        // Broadcast PeerJoined to all existing peers in the room
        let new_peer_info = PeerInfo {
            client_id,
            addr: peer_addr,
            is_host,
        };
        let peer_joined_msg = RendezvousMessage::PeerJoined { peer: new_peer_info };
        if let Ok(bytes) = peer_joined_msg.encode() {
            for target_addr in existing_peer_addrs {
                if let Err(error) = self.socket.send_to(&bytes, target_addr).await {
                    warn!(%error, %room_token, %target_addr, "failed to send PeerJoined notification");
                }
            }
        }
    }

    /// Forwards a `Signal` (passed through as its raw encoded bytes) to `to`,
    /// only when both ends are registered in the room and the sender really
    /// is `from`'s registered address -- the hub must not become an open
    /// UDP reflector.
    async fn handle_signal(&self, room_token: Uuid, from: Uuid, to: Uuid, raw: &[u8], peer_addr: SocketAddr) {
        let target = {
            let rooms = self.rooms.lock().await;
            let Some(room) = rooms.get(&room_token) else {
                return;
            };
            match (room.get(&from), room.get(&to)) {
                (Some(sender), Some(target)) if sender.addr == peer_addr => target.addr,
                _ => {
                    debug!(%room_token, %from, %to, %peer_addr, "dropping signal from or to an unregistered peer");
                    return;
                }
            }
        };
        if let Err(error) = self.socket.send_to(raw, target).await {
            debug!(%error, %room_token, %target, "failed to forward signal");
        }
    }

    async fn handle_heartbeat(&self, room_token: Uuid, client_id: Uuid, peer_addr: SocketAddr) {
        {
            let mut rooms = self.rooms.lock().await;
            match rooms.get_mut(&room_token).and_then(|room| room.get_mut(&client_id)) {
                // With tickets, identity was proven at registration from that
                // address; a heartbeat from anywhere else could be a hijack.
                Some(peer) if self.ticket_secret.is_some() && peer.addr != peer_addr => {
                    debug!(%room_token, %client_id, %peer_addr, "heartbeat from an unregistered address");
                }
                Some(peer) => {
                    peer.last_seen = Instant::now();
                    peer.addr = peer_addr;
                    debug!(%room_token, %client_id, %peer_addr, "updated heartbeat last_seen");
                    return;
                }
                None => debug!(%room_token, %client_id, "heartbeat for an unregistered peer"),
            }
        }
        // This hub restarted (and forgot the room) or the peer's public
        // address changed: have it register again, proving who it is. The
        // reply is smaller than the heartbeat, so it can't amplify anything.
        if let Ok(bytes) = RendezvousMessage::RegisterAgain.encode() {
            let _ = self.socket.send_to(&bytes, peer_addr).await;
        }
    }

    async fn cleanup_expired(&self) {
        struct LeftNotification {
            remaining_addrs: Vec<SocketAddr>,
            expired_client_id: Uuid,
            room_token: Uuid,
        }

        let mut notifications = Vec::new();
        {
            let mut rooms = self.rooms.lock().await;
            let now = Instant::now();
            let ttl = self.ttl;

            rooms.retain(|room_token, room| {
                let mut expired_ids = Vec::new();

                room.retain(|client_id, state| {
                    let expired = now.duration_since(state.last_seen) >= ttl;
                    if expired {
                        info!(%room_token, %client_id, "purging expired peer from room");
                        expired_ids.push(*client_id);
                    }
                    !expired
                });

                if !expired_ids.is_empty() && !room.is_empty() {
                    let remaining_addrs: Vec<SocketAddr> = room.values().map(|s| s.addr).collect();
                    for expired_id in expired_ids {
                        notifications.push(LeftNotification {
                            remaining_addrs: remaining_addrs.clone(),
                            expired_client_id: expired_id,
                            room_token: *room_token,
                        });
                    }
                }

                !room.is_empty()
            });
        }

        for notif in notifications {
            let msg = RendezvousMessage::PeerLeft {
                client_id: notif.expired_client_id,
            };
            if let Ok(bytes) = msg.encode() {
                for target_addr in notif.remaining_addrs {
                    if let Err(error) = self.socket.send_to(&bytes, target_addr).await {
                        warn!(%error, room_token = %notif.room_token, %target_addr, "failed to send PeerLeft notification");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    async fn recv_msg(socket: &UdpSocket) -> RendezvousMessage {
        let mut buf = [0_u8; 2048];
        let (n, _) = timeout(Duration::from_secs(2), socket.recv_from(&mut buf))
            .await
            .expect("timed out waiting for udp datagram")
            .expect("recv_from error");
        RendezvousMessage::decode(&buf[..n]).expect("decode rendezvous message")
    }

    #[tokio::test]
    async fn three_participants_registration_and_presence_notifications() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind server");
        let server_addr = server_socket.local_addr().expect("server addr");
        let rendezvous = Arc::new(UdpRendezvous::new(server_socket));
        rendezvous.spawn();

        let room_token = Uuid::new_v4();

        let client1_id = Uuid::new_v4();
        let client1_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind client1");
        let client1_addr = client1_socket.local_addr().expect("client1 addr");

        let client2_id = Uuid::new_v4();
        let client2_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind client2");
        let client2_addr = client2_socket.local_addr().expect("client2 addr");

        let client3_id = Uuid::new_v4();
        let client3_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind client3");
        let client3_addr = client3_socket.local_addr().expect("client3 addr");

        // 1. Client 1 (Host) registers
        let reg1 = RendezvousMessage::Register {
            room_token,
            client_id: client1_id,
            is_host: true,
        }
        .encode()
        .expect("encode reg1");
        client1_socket.send_to(&reg1, server_addr).await.expect("send reg1");

        // Client 1 receives PeerList with 0 peers
        let msg = recv_msg(&client1_socket).await;
        assert_eq!(msg, RendezvousMessage::PeerList { peers: vec![] });

        // 2. Client 2 (Joiner 1) registers
        let reg2 = RendezvousMessage::Register {
            room_token,
            client_id: client2_id,
            is_host: false,
        }
        .encode()
        .expect("encode reg2");
        client2_socket.send_to(&reg2, server_addr).await.expect("send reg2");

        // Client 2 receives PeerList with Client 1
        let msg2 = recv_msg(&client2_socket).await;
        assert_eq!(
            msg2,
            RendezvousMessage::PeerList {
                peers: vec![PeerInfo {
                    client_id: client1_id,
                    addr: client1_addr,
                    is_host: true,
                }]
            }
        );

        // Client 1 receives PeerJoined for Client 2
        let msg1_joined = recv_msg(&client1_socket).await;
        assert_eq!(
            msg1_joined,
            RendezvousMessage::PeerJoined {
                peer: PeerInfo {
                    client_id: client2_id,
                    addr: client2_addr,
                    is_host: false,
                }
            }
        );

        // 3. Client 3 (Joiner 2) registers
        let reg3 = RendezvousMessage::Register {
            room_token,
            client_id: client3_id,
            is_host: false,
        }
        .encode()
        .expect("encode reg3");
        client3_socket.send_to(&reg3, server_addr).await.expect("send reg3");

        // Client 3 receives PeerList with Client 1 and Client 2
        let msg3 = recv_msg(&client3_socket).await;
        if let RendezvousMessage::PeerList { mut peers } = msg3 {
            peers.sort_by_key(|p| p.client_id);
            let mut expected = vec![
                PeerInfo {
                    client_id: client1_id,
                    addr: client1_addr,
                    is_host: true,
                },
                PeerInfo {
                    client_id: client2_id,
                    addr: client2_addr,
                    is_host: false,
                },
            ];
            expected.sort_by_key(|p| p.client_id);
            assert_eq!(peers, expected);
        } else {
            panic!("expected PeerList for client 3, got {:?}", msg3);
        }

        // Client 1 and Client 2 both receive PeerJoined for Client 3
        let msg1_joined2 = recv_msg(&client1_socket).await;
        assert_eq!(
            msg1_joined2,
            RendezvousMessage::PeerJoined {
                peer: PeerInfo {
                    client_id: client3_id,
                    addr: client3_addr,
                    is_host: false,
                }
            }
        );

        let msg2_joined2 = recv_msg(&client2_socket).await;
        assert_eq!(
            msg2_joined2,
            RendezvousMessage::PeerJoined {
                peer: PeerInfo {
                    client_id: client3_id,
                    addr: client3_addr,
                    is_host: false,
                }
            }
        );
    }

    #[tokio::test]
    async fn probe_is_answered_with_the_observed_source_address() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind server");
        let server_addr = server_socket.local_addr().expect("server addr");
        Arc::new(UdpRendezvous::new(server_socket)).spawn();

        let client = UdpSocket::bind("127.0.0.1:0").await.expect("bind client");
        let client_addr = client.local_addr().expect("client addr");
        let probe = RendezvousMessage::Probe.encode().expect("encode probe");
        client.send_to(&probe, server_addr).await.expect("send probe");

        assert_eq!(
            recv_msg(&client).await,
            RendezvousMessage::ProbeReply { observed: client_addr }
        );
    }

    #[tokio::test]
    async fn heartbeat_it_cannot_place_asks_the_peer_to_register_again() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind server");
        let server_addr = server_socket.local_addr().expect("server addr");
        Arc::new(UdpRendezvous::new(server_socket)).spawn();

        // As after a restart: the hub never saw this room.
        let client = UdpSocket::bind("127.0.0.1:0").await.expect("bind client");
        let (room_token, client_id) = (Uuid::new_v4(), Uuid::new_v4());
        let heartbeat = RendezvousMessage::Heartbeat { room_token, client_id };
        client.send_to(&heartbeat.encode().unwrap(), server_addr).await.unwrap();
        assert_eq!(recv_msg(&client).await, RendezvousMessage::RegisterAgain);

        // Once registered, heartbeats are taken silently.
        let reg = RendezvousMessage::Register {
            room_token,
            client_id,
            is_host: true,
        };
        client.send_to(&reg.encode().unwrap(), server_addr).await.unwrap();
        assert!(matches!(recv_msg(&client).await, RendezvousMessage::PeerList { .. }));
        client.send_to(&heartbeat.encode().unwrap(), server_addr).await.unwrap();
        let mut buf = [0_u8; 64];
        assert!(timeout(Duration::from_millis(200), client.recv_from(&mut buf))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn signal_is_forwarded_only_between_registered_room_members() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind server");
        let server_addr = server_socket.local_addr().expect("server addr");
        Arc::new(UdpRendezvous::new(server_socket)).spawn();

        let room_token = Uuid::new_v4();
        let (a_id, b_id) = (Uuid::new_v4(), Uuid::new_v4());
        let a = UdpSocket::bind("127.0.0.1:0").await.expect("bind a");
        let b = UdpSocket::bind("127.0.0.1:0").await.expect("bind b");
        for (socket, client_id) in [(&a, a_id), (&b, b_id)] {
            let reg = RendezvousMessage::Register {
                room_token,
                client_id,
                is_host: false,
            }
            .encode()
            .unwrap();
            socket.send_to(&reg, server_addr).await.unwrap();
            assert!(matches!(recv_msg(socket).await, RendezvousMessage::PeerList { .. }));
        }
        assert!(matches!(recv_msg(&a).await, RendezvousMessage::PeerJoined { .. }));

        let signal = RendezvousMessage::Signal {
            room_token,
            from: a_id,
            to: b_id,
            payload: b"offer".to_vec(),
        };
        a.send_to(&signal.encode().unwrap(), server_addr).await.unwrap();
        assert_eq!(recv_msg(&b).await, signal);

        // Claiming to be `a` from another socket is dropped.
        let intruder = UdpSocket::bind("127.0.0.1:0").await.expect("bind intruder");
        intruder.send_to(&signal.encode().unwrap(), server_addr).await.unwrap();
        let mut buf = [0_u8; 512];
        assert!(timeout(Duration::from_millis(200), b.recv_from(&mut buf))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn different_registration_order_and_heartbeat_updates() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind server");
        let server_addr = server_socket.local_addr().expect("server addr");
        let rendezvous = Arc::new(UdpRendezvous::new(server_socket));
        rendezvous.spawn();

        let room_token = Uuid::new_v4();

        let c1_id = Uuid::new_v4(); // Joiner 1
        let c1_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind c1");
        let c1_addr = c1_socket.local_addr().expect("c1 addr");

        let c2_id = Uuid::new_v4(); // Joiner 2
        let c2_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind c2");
        let c2_addr = c2_socket.local_addr().expect("c2 addr");

        let host_id = Uuid::new_v4(); // Host (registers last)
        let host_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind host");
        let host_addr = host_socket.local_addr().expect("host addr");

        // 1. Joiner 1 registers
        let reg1 = RendezvousMessage::Register {
            room_token,
            client_id: c1_id,
            is_host: false,
        }
        .encode()
        .unwrap();
        c1_socket.send_to(&reg1, server_addr).await.unwrap();

        let msg1 = recv_msg(&c1_socket).await;
        assert_eq!(msg1, RendezvousMessage::PeerList { peers: vec![] });

        // 2. Joiner 2 registers
        let reg2 = RendezvousMessage::Register {
            room_token,
            client_id: c2_id,
            is_host: false,
        }
        .encode()
        .unwrap();
        c2_socket.send_to(&reg2, server_addr).await.unwrap();

        let msg2 = recv_msg(&c2_socket).await;
        assert_eq!(
            msg2,
            RendezvousMessage::PeerList {
                peers: vec![PeerInfo {
                    client_id: c1_id,
                    addr: c1_addr,
                    is_host: false,
                }]
            }
        );

        let msg1_joined = recv_msg(&c1_socket).await;
        assert_eq!(
            msg1_joined,
            RendezvousMessage::PeerJoined {
                peer: PeerInfo {
                    client_id: c2_id,
                    addr: c2_addr,
                    is_host: false,
                }
            }
        );

        // 3. Host registers last
        let reg_host = RendezvousMessage::Register {
            room_token,
            client_id: host_id,
            is_host: true,
        }
        .encode()
        .unwrap();
        host_socket.send_to(&reg_host, server_addr).await.unwrap();

        let msg_host = recv_msg(&host_socket).await;
        if let RendezvousMessage::PeerList { mut peers } = msg_host {
            peers.sort_by_key(|p| p.client_id);
            let mut expected = vec![
                PeerInfo {
                    client_id: c1_id,
                    addr: c1_addr,
                    is_host: false,
                },
                PeerInfo {
                    client_id: c2_id,
                    addr: c2_addr,
                    is_host: false,
                },
            ];
            expected.sort_by_key(|p| p.client_id);
            assert_eq!(peers, expected);
        } else {
            panic!("expected PeerList for host");
        }

        // c1 and c2 receive PeerJoined for host
        let c1_joined_host = recv_msg(&c1_socket).await;
        assert_eq!(
            c1_joined_host,
            RendezvousMessage::PeerJoined {
                peer: PeerInfo {
                    client_id: host_id,
                    addr: host_addr,
                    is_host: true,
                }
            }
        );

        let c2_joined_host = recv_msg(&c2_socket).await;
        assert_eq!(
            c2_joined_host,
            RendezvousMessage::PeerJoined {
                peer: PeerInfo {
                    client_id: host_id,
                    addr: host_addr,
                    is_host: true,
                }
            }
        );

        // 4. Send Heartbeat from c1; verify it does NOT trigger PeerJoined to others
        let hb = RendezvousMessage::Heartbeat {
            room_token,
            client_id: c1_id,
        }
        .encode()
        .unwrap();
        c1_socket.send_to(&hb, server_addr).await.unwrap();

        let mut buf = [0_u8; 512];
        let timeout_res = timeout(Duration::from_millis(150), c2_socket.recv_from(&mut buf)).await;
        assert!(
            timeout_res.is_err(),
            "heartbeat should not broadcast PeerJoined to existing peers"
        );
    }

    #[tokio::test]
    async fn stale_heartbeat_triggers_peer_left_without_affecting_others() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind server");
        let server_addr = server_socket.local_addr().expect("server addr");

        // TTL of 200ms for testing cleanup
        let rendezvous = Arc::new(UdpRendezvous::with_ttl(server_socket, Duration::from_millis(200)));
        rendezvous.spawn();

        let room_token = Uuid::new_v4();

        let c1_id = Uuid::new_v4();
        let c1_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        let c2_id = Uuid::new_v4();
        let c2_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        let c3_id = Uuid::new_v4();
        let c3_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        // 1. Register c1
        let reg1 = RendezvousMessage::Register {
            room_token,
            client_id: c1_id,
            is_host: true,
        }
        .encode()
        .unwrap();
        c1_socket.send_to(&reg1, server_addr).await.unwrap();
        let msg1 = recv_msg(&c1_socket).await;
        assert_eq!(msg1, RendezvousMessage::PeerList { peers: vec![] });

        // 2. Register c2
        let reg2 = RendezvousMessage::Register {
            room_token,
            client_id: c2_id,
            is_host: false,
        }
        .encode()
        .unwrap();
        c2_socket.send_to(&reg2, server_addr).await.unwrap();
        let msg2 = recv_msg(&c2_socket).await;
        assert!(matches!(msg2, RendezvousMessage::PeerList { .. }));
        let msg1_j = recv_msg(&c1_socket).await;
        assert!(matches!(msg1_j, RendezvousMessage::PeerJoined { .. }));

        // 3. Register c3
        let reg3 = RendezvousMessage::Register {
            room_token,
            client_id: c3_id,
            is_host: false,
        }
        .encode()
        .unwrap();
        c3_socket.send_to(&reg3, server_addr).await.unwrap();
        let msg3 = recv_msg(&c3_socket).await;
        assert!(matches!(msg3, RendezvousMessage::PeerList { .. }));
        let _ = recv_msg(&c1_socket).await; // c1 receives PeerJoined(c3)
        let _ = recv_msg(&c2_socket).await; // c2 receives PeerJoined(c3)

        // All sockets are clean and drained.
        // c1 and c3 continue sending heartbeats every 40ms for 320ms; c2 STOPS sending heartbeats
        for _ in 0..8 {
            tokio::time::sleep(Duration::from_millis(40)).await;

            let hb1 = RendezvousMessage::Heartbeat {
                room_token,
                client_id: c1_id,
            }
            .encode()
            .unwrap();
            c1_socket.send_to(&hb1, server_addr).await.unwrap();

            let hb3 = RendezvousMessage::Heartbeat {
                room_token,
                client_id: c3_id,
            }
            .encode()
            .unwrap();
            c3_socket.send_to(&hb3, server_addr).await.unwrap();
        }

        // Both c1 and c3 should receive PeerLeft for c2
        let left1 = recv_msg(&c1_socket).await;
        assert_eq!(left1, RendezvousMessage::PeerLeft { client_id: c2_id });

        let left3 = recv_msg(&c3_socket).await;
        assert_eq!(left3, RendezvousMessage::PeerLeft { client_id: c2_id });
    }
}
