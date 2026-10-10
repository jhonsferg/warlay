use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, info, warn};
use uuid::Uuid;

use warlay_protocol::RendezvousMessage;

use crate::p2p_tunnel::{punch_and_connect, P2pTunnelHandle};

const LINK_OFFER_MAGIC: &[u8] = b"WARLAY_LINK_OFFER_V1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LinkOffer {
    room_token: Uuid,
    client_id: Uuid,
    p2p_addr: SocketAddr,
}

impl LinkOffer {
    fn encode(&self) -> Result<Vec<u8>, bincode::Error> {
        let mut buf = Vec::with_capacity(LINK_OFFER_MAGIC.len() + 64);
        buf.extend_from_slice(LINK_OFFER_MAGIC);
        let payload = bincode::serialize(self)?;
        buf.extend_from_slice(&payload);
        Ok(buf)
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() <= LINK_OFFER_MAGIC.len() || &bytes[..LINK_OFFER_MAGIC.len()] != LINK_OFFER_MAGIC {
            return None;
        }
        bincode::deserialize(&bytes[LINK_OFFER_MAGIC.len()..]).ok()
    }
}

#[derive(Debug, Clone)]
pub enum PathToHost {
    Direct(Arc<P2pTunnelHandle>),
    ViaPeer(Uuid, Arc<P2pTunnelHandle>),
}

#[derive(Debug, Clone)]
pub enum PeerLinkState {
    NotAttempted,
    Connecting,
    Connected(Arc<P2pTunnelHandle>),
    Failed,
}

#[derive(Debug, Clone)]
pub struct PeerEntry {
    pub client_id: Uuid,
    pub addr: SocketAddr,
    pub p2p_addr: Option<SocketAddr>,
    pub is_host: bool,
    pub state: PeerLinkState,
}

pub struct PeerMeshSharedState {
    pub own_client_id: Uuid,
    pub is_host: bool,
    pub room_token: Uuid,
    pub peers: HashMap<Uuid, PeerEntry>,
    pub pending_sockets: HashMap<Uuid, SocketAddr>,
    pub bridged_peers: HashSet<Uuid>,
    pub datagram_peers: HashSet<Uuid>,
    pub bridge_tasks: Vec<JoinHandle<()>>,
    pub datagram_tasks: Vec<JoinHandle<()>>,
    pub own_registration_addr: Option<SocketAddr>,
    pub datagram_tx: tokio::sync::mpsc::UnboundedSender<(Uuid, Vec<u8>)>,
}

pub struct PeerMeshHandle {
    task: JoinHandle<()>,
    state: Arc<std::sync::RwLock<PeerMeshSharedState>>,
    datagram_rx: std::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<(Uuid, Vec<u8>)>>,
}

impl PeerMeshHandle {
    pub fn abort(&self) {
        self.task.abort();
        if let Ok(mut st) = self.state.write() {
            for task in st.bridge_tasks.drain(..) {
                task.abort();
            }
            for task in st.datagram_tasks.drain(..) {
                task.abort();
            }
        }
    }

    /// The address this mesh's registration socket is bound to — the same
    /// address peers learn about it through `PeerList`/`PeerJoined`. Test
    /// fixtures that stand up a mock rendezvous for one node need this to
    /// tell that node about another real mesh instance's actual address,
    /// since a mock can't otherwise know it. `None` until registration
    /// with the real rendezvous server completes.
    pub fn own_registration_addr(&self) -> Option<SocketAddr> {
        self.state.read().ok()?.own_registration_addr
    }

    pub fn connected_peers(&self) -> Vec<(Uuid, Arc<P2pTunnelHandle>)> {
        let Ok(st) = self.state.read() else {
            return Vec::new();
        };

        st.peers
            .values()
            .filter_map(|p| {
                if let PeerLinkState::Connected(ref handle) = p.state {
                    Some((p.client_id, handle.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn best_path_to_host(&self) -> Option<PathToHost> {
        let state = self.state.read().ok()?;

        // 1. Direct connection to the host?
        for peer in state.peers.values() {
            if peer.is_host {
                if let PeerLinkState::Connected(ref handle) = peer.state {
                    return Some(PathToHost::Direct(handle.clone()));
                }
            }
        }

        // 2. Via peer (bridge)?
        for peer in state.peers.values() {
            if !peer.is_host {
                if let PeerLinkState::Connected(ref handle) = peer.state {
                    return Some(PathToHost::ViaPeer(peer.client_id, handle.clone()));
                }
            }
        }

        None
    }

    pub fn send_datagram_to(&self, peer_id: Uuid, data: Vec<u8>) -> bool {
        let Ok(st) = self.state.read() else {
            return false;
        };

        if let Some(peer) = st.peers.get(&peer_id) {
            if let PeerLinkState::Connected(ref handle) = peer.state {
                return handle.connection.send_datagram(data.into()).is_ok();
            }
        }

        false
    }

    pub fn poll_datagrams(&self) -> Vec<(Uuid, Vec<u8>)> {
        let mut datagrams = Vec::new();
        if let Ok(mut rx) = self.datagram_rx.lock() {
            while let Ok(item) = rx.try_recv() {
                datagrams.push(item);
            }
        }
        datagrams
    }

    /// A snapshot of every peer this mesh knows about and how far its
    /// direct-link negotiation with each has gotten — for the "Agente"
    /// diagnostic panel. Deliberately a plain tuple (not `PeerEntry`
    /// itself): `PeerLinkState::Connected` carries a live
    /// `Arc<P2pTunnelHandle>` that has no business leaving this crate, and
    /// the caller (the daemon) maps this into `protocol::ipc`'s own
    /// serializable DTO.
    pub fn peer_statuses(&self) -> Vec<(Uuid, bool, Option<SocketAddr>, &'static str)> {
        let Ok(st) = self.state.read() else {
            return Vec::new();
        };

        st.peers
            .values()
            .map(|p| {
                let tag = match p.state {
                    PeerLinkState::NotAttempted => "not_attempted",
                    PeerLinkState::Connecting => "connecting",
                    PeerLinkState::Connected(_) => "connected",
                    PeerLinkState::Failed => "failed",
                };
                (p.client_id, p.is_host, p.p2p_addr, tag)
            })
            .collect()
    }

    /// Resets any peer stuck in `PeerLinkState::Failed` back to
    /// `NotAttempted` — the mesh's own `attempt_interval` (every 3s, see
    /// `run_peer_mesh`) picks it up on its next tick and retries, same as
    /// it already does automatically; this just makes that happen on
    /// demand instead of waiting.
    pub fn retry_failed_peers(&self) {
        let Ok(mut st) = self.state.write() else {
            return;
        };
        for peer in st.peers.values_mut() {
            if matches!(peer.state, PeerLinkState::Failed) {
                peer.state = PeerLinkState::NotAttempted;
            }
        }
    }
}

pub struct PeerMesh;

impl PeerMesh {
    pub fn spawn(rendezvous_addr: SocketAddr, room_token: Uuid, own_client_id: Uuid, is_host: bool) -> PeerMeshHandle {
        let (datagram_tx, datagram_rx) = tokio::sync::mpsc::unbounded_channel();
        let state = Arc::new(std::sync::RwLock::new(PeerMeshSharedState {
            own_client_id,
            is_host,
            room_token,
            peers: HashMap::new(),
            pending_sockets: HashMap::new(),
            bridged_peers: HashSet::new(),
            datagram_peers: HashSet::new(),
            bridge_tasks: Vec::new(),
            datagram_tasks: Vec::new(),
            own_registration_addr: None,
            datagram_tx,
        }));

        let state_task = state.clone();
        let task = tokio::spawn(async move {
            run_peer_mesh(rendezvous_addr, room_token, own_client_id, is_host, state_task).await;
        });

        PeerMeshHandle {
            task,
            state,
            datagram_rx: std::sync::Mutex::new(datagram_rx),
        }
    }
}

async fn run_peer_mesh(
    rendezvous_addr: SocketAddr,
    room_token: Uuid,
    own_client_id: Uuid,
    is_host: bool,
    state: Arc<std::sync::RwLock<PeerMeshSharedState>>,
) {
    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(err) => {
            warn!(%err, "failed to bind UDP socket for PeerMesh");
            return;
        }
    };

    if let Ok(local_addr) = socket.local_addr() {
        if let Ok(mut st) = state.write() {
            st.own_registration_addr = Some(local_addr);
        }
    }

    let reg_msg = match (RendezvousMessage::Register {
        room_token,
        client_id: own_client_id,
        is_host,
    })
    .encode()
    {
        Ok(b) => b,
        Err(err) => {
            warn!(%err, "failed to encode Register message in PeerMesh");
            return;
        }
    };

    let mut buf = [0_u8; 2048];
    let mut registered = false;

    while !registered {
        let _ = socket.send_to(&reg_msg, rendezvous_addr).await;
        match timeout(Duration::from_millis(300), socket.recv_from(&mut buf)).await {
            Ok(Ok((len, src))) if src == rendezvous_addr => {
                if let Ok(msg) = RendezvousMessage::decode(&buf[..len]) {
                    if let RendezvousMessage::PeerList { peers } = msg {
                        registered = true;
                        let mut st = state.write().unwrap();
                        for p in peers {
                            if p.client_id != own_client_id {
                                st.peers.entry(p.client_id).or_insert(PeerEntry {
                                    client_id: p.client_id,
                                    addr: p.addr,
                                    p2p_addr: None,
                                    is_host: p.is_host,
                                    state: PeerLinkState::NotAttempted,
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    info!(%room_token, %own_client_id, is_host, "registered presence with rendezvous server in PeerMesh");
    let socket = Arc::new(socket);

    trigger_connection_attempts(room_token, own_client_id, is_host, &state, &socket);

    let heartbeat_msg = match (RendezvousMessage::Heartbeat {
        room_token,
        client_id: own_client_id,
    })
    .encode()
    {
        Ok(b) => b,
        Err(_) => return,
    };

    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(15));
    heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut attempt_interval = tokio::time::interval(Duration::from_secs(3));
    attempt_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                let _ = socket.send_to(&heartbeat_msg, rendezvous_addr).await;
            }
            _ = attempt_interval.tick() => {
                trigger_connection_attempts(room_token, own_client_id, is_host, &state, &socket);
            }
            recv_res = socket.recv_from(&mut buf) => {
                match recv_res {
                    Ok((len, src)) if src == rendezvous_addr => {
                        if let Ok(msg) = RendezvousMessage::decode(&buf[..len]) {
                            match msg {
                                RendezvousMessage::PeerList { peers } => {
                                    let mut st = state.write().unwrap();
                                    for p in peers {
                                        if p.client_id != own_client_id {
                                            st.peers.entry(p.client_id).or_insert(PeerEntry {
                                                client_id: p.client_id,
                                                addr: p.addr,
                                                p2p_addr: None,
                                                is_host: p.is_host,
                                                state: PeerLinkState::NotAttempted,
                                            });
                                        }
                                    }
                                    drop(st);
                                    trigger_connection_attempts(room_token, own_client_id, is_host, &state, &socket);
                                }
                                RendezvousMessage::PeerJoined { peer } => {
                                    if peer.client_id != own_client_id {
                                        info!(%room_token, peer_id = %peer.client_id, peer_addr = %peer.addr, "PeerJoined notification received");
                                        let mut st = state.write().unwrap();
                                        st.peers.entry(peer.client_id)
                                            .and_modify(|e| e.addr = peer.addr)
                                            .or_insert(PeerEntry {
                                                client_id: peer.client_id,
                                                addr: peer.addr,
                                                p2p_addr: None,
                                                is_host: peer.is_host,
                                                state: PeerLinkState::NotAttempted,
                                            });
                                        drop(st);
                                        trigger_connection_attempts(room_token, own_client_id, is_host, &state, &socket);
                                    }
                                }
                                RendezvousMessage::PeerLeft { client_id } => {
                                    info!(%room_token, %client_id, "PeerLeft notification received");
                                    let mut st = state.write().unwrap();
                                    st.peers.remove(&client_id);
                                    st.pending_sockets.remove(&client_id);
                                    st.bridged_peers.remove(&client_id);
                                    st.datagram_peers.remove(&client_id);
                                }
                                _ => {}
                            }
                        }
                    }
                    Ok((len, src)) => {
                        if let Some(offer) = LinkOffer::decode(&buf[..len]) {
                            if offer.room_token == room_token && offer.client_id != own_client_id {
                                let remote_p2p = if offer.p2p_addr.ip().is_unspecified() {
                                    SocketAddr::new(src.ip(), offer.p2p_addr.port())
                                } else {
                                    offer.p2p_addr
                                };
                                debug!(%room_token, peer_id = %offer.client_id, %remote_p2p, "received LinkOffer from peer");
                                let mut st = state.write().unwrap();
                                if let Some(entry) = st.peers.get_mut(&offer.client_id) {
                                    entry.p2p_addr = Some(remote_p2p);
                                } else {
                                    st.peers.insert(offer.client_id, PeerEntry {
                                        client_id: offer.client_id,
                                        addr: src,
                                        p2p_addr: Some(remote_p2p),
                                        is_host: false,
                                        state: PeerLinkState::NotAttempted,
                                    });
                                }
                                drop(st);
                                trigger_connection_attempts(room_token, own_client_id, is_host, &state, &socket);
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
        }
    }
}

fn trigger_connection_attempts(
    room_token: Uuid,
    own_client_id: Uuid,
    is_host: bool,
    state: &Arc<std::sync::RwLock<PeerMeshSharedState>>,
    reg_socket: &Arc<UdpSocket>,
) {
    let mut targets = Vec::new();
    {
        let mut st = state.write().unwrap();
        for peer in st.peers.values_mut() {
            match peer.state {
                PeerLinkState::NotAttempted | PeerLinkState::Failed => {
                    peer.state = PeerLinkState::Connecting;
                    peer.p2p_addr = None;
                    targets.push((peer.client_id, peer.addr, peer.is_host));
                }
                _ => {}
            }
        }
    }

    if targets.is_empty() {
        return;
    }

    // Prioritize peer with is_host == true first
    targets.sort_by_key(|(_, _, peer_is_host)| !peer_is_host);

    for (peer_id, peer_reg_addr, peer_is_host) in targets {
        let state_clone = state.clone();
        let reg_socket_clone = reg_socket.clone();

        tokio::spawn(async move {
            let local_socket = match UdpSocket::bind("0.0.0.0:0").await {
                Ok(s) => s,
                Err(_) => {
                    let mut st = state_clone.write().unwrap();
                    if let Some(p) = st.peers.get_mut(&peer_id) {
                        p.state = PeerLinkState::Failed;
                    }
                    return;
                }
            };

            let local_p2p_addr = match local_socket.local_addr() {
                Ok(a) => a,
                Err(_) => {
                    let mut st = state_clone.write().unwrap();
                    if let Some(p) = st.peers.get_mut(&peer_id) {
                        p.state = PeerLinkState::Failed;
                    }
                    return;
                }
            };

            let offer = LinkOffer {
                room_token,
                client_id: own_client_id,
                p2p_addr: local_p2p_addr,
            };

            let offer_bytes = match offer.encode() {
                Ok(b) => b,
                Err(_) => return,
            };

            let offer_deadline = tokio::time::Instant::now() + Duration::from_millis(2500);
            let mut target_p2p_addr = None;

            while tokio::time::Instant::now() < offer_deadline {
                let _ = reg_socket_clone.send_to(&offer_bytes, peer_reg_addr).await;

                {
                    let st = state_clone.read().unwrap();
                    if let Some(peer) = st.peers.get(&peer_id) {
                        if let Some(remote_addr) = peer.p2p_addr {
                            target_p2p_addr = Some(remote_addr);
                            break;
                        }
                    }
                }

                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            for _ in 0..3 {
                let _ = reg_socket_clone.send_to(&offer_bytes, peer_reg_addr).await;
            }

            let remote_p2p_addr = match target_p2p_addr {
                Some(addr) => addr,
                None => {
                    debug!(%room_token, %peer_id, %peer_reg_addr, "timed out waiting for LinkOffer from peer");
                    let mut st = state_clone.write().unwrap();
                    if let Some(p) = st.peers.get_mut(&peer_id) {
                        p.state = PeerLinkState::Failed;
                    }
                    return;
                }
            };

            let act_as_quic_server = if is_host != peer_is_host {
                is_host
            } else {
                own_client_id < peer_id
            };

            let res = timeout(
                Duration::from_secs(4),
                punch_and_connect(local_socket, remote_p2p_addr, room_token, act_as_quic_server),
            )
            .await;

            let mut st = state_clone.write().unwrap();
            match res {
                Ok(Ok(handle)) => {
                    info!(%room_token, %peer_id, %remote_p2p_addr, "successfully connected P2P link to peer");
                    let handle_arc = Arc::new(handle);
                    if let Some(p) = st.peers.get_mut(&peer_id) {
                        p.state = PeerLinkState::Connected(handle_arc.clone());
                    }

                    if st.datagram_peers.insert(peer_id) {
                        let handle_task = handle_arc.clone();
                        let tx = st.datagram_tx.clone();
                        let datagram_task = tokio::spawn(async move {
                            run_datagram_listener_for_peer(peer_id, handle_task, tx).await;
                        });
                        st.datagram_tasks.push(datagram_task);
                    }

                    // Only spawn a bridge-relay watcher if we ourselves
                    // already have a live connection to the host -- a node
                    // with no path to host has nothing to bridge with, and
                    // more importantly must NOT compete for reads on this
                    // same link: if the peer we just connected to is the
                    // one *we* are using as *our own* best_path_to_host()
                    // (see tcp_tunnel_client.rs's ViaPeer consumption),
                    // spawning a watcher here would race our own bridging
                    // code for every byte the peer sends back -- exactly
                    // the reply traffic our own consumer is waiting for.
                    let we_have_host_link = st
                        .peers
                        .values()
                        .any(|p| p.is_host && matches!(p.state, PeerLinkState::Connected(_)));
                    if !is_host && !peer_is_host && we_have_host_link && st.bridged_peers.insert(peer_id) {
                        let state_task = state_clone.clone();
                        let handle_task = handle_arc;
                        let bridge_task = tokio::spawn(async move {
                            run_bridge_for_peer(state_task, peer_id, handle_task).await;
                        });
                        st.bridge_tasks.push(bridge_task);
                    }
                }
                _ => {
                    debug!(%room_token, %peer_id, %remote_p2p_addr, "P2P connection attempt to peer failed or timed out");
                    if let Some(p) = st.peers.get_mut(&peer_id) {
                        p.state = PeerLinkState::Failed;
                    }
                }
            }
        });
    }
}

async fn run_bridge_for_peer(
    state: Arc<std::sync::RwLock<PeerMeshSharedState>>,
    peer_id: Uuid,
    handle_peer: Arc<P2pTunnelHandle>,
) {
    let mut first_buf = [0_u8; 8192];
    let n = {
        let mut recv_guard = handle_peer.recv.lock().await;
        match recv_guard.read(&mut first_buf).await {
            Ok(Some(n)) if n > 0 => n,
            _ => return,
        }
    };

    debug!(%peer_id, n, "bridge task received initial bytes from peer, connecting to host");

    let host_handle = match wait_for_host_handle(&state, Duration::from_secs(5)).await {
        Some(h) => h,
        None => {
            warn!(%peer_id, "bridge task failed to obtain host connection within timeout");
            return;
        }
    };

    let (mut host_send, mut host_recv) = match host_handle.connection.open_bi().await {
        Ok(s) => s,
        Err(err) => {
            warn!(%err, %peer_id, "failed to open bi stream to host in bridge task");
            return;
        }
    };

    if host_send.write_all(&[0x01]).await.is_err() {
        return;
    }
    if host_send.write_all(&first_buf[..n]).await.is_err() {
        return;
    }

    let handle_peer_1 = handle_peer.clone();
    let peer_to_host = tokio::spawn(async move {
        let mut buf = [0_u8; 8192];
        loop {
            let n = match handle_peer_1.recv.lock().await.read(&mut buf).await {
                Ok(Some(n)) if n > 0 => n,
                Ok(Some(_)) => continue,
                _ => break,
            };
            if host_send.write_all(&buf[..n]).await.is_err() {
                break;
            }
        }
    });

    let handle_peer_2 = handle_peer.clone();
    let host_to_peer = tokio::spawn(async move {
        let mut buf = [0_u8; 8192];
        loop {
            let n = match host_recv.read(&mut buf).await {
                Ok(Some(n)) if n > 0 => n,
                Ok(Some(_)) => continue,
                _ => break,
            };
            if handle_peer_2.send.lock().await.write_all(&buf[..n]).await.is_err() {
                break;
            }
        }
    });

    let _ = tokio::join!(peer_to_host, host_to_peer);
}

async fn run_datagram_listener_for_peer(
    peer_id: Uuid,
    handle_peer: Arc<P2pTunnelHandle>,
    tx: tokio::sync::mpsc::UnboundedSender<(Uuid, Vec<u8>)>,
) {
    loop {
        match handle_peer.connection.read_datagram().await {
            Ok(bytes) => {
                if tx.send((peer_id, bytes.to_vec())).is_err() {
                    break;
                }
            }
            Err(err) => {
                debug!(%peer_id, %err, "datagram listener connection closed or read error");
                break;
            }
        }
    }
}

async fn wait_for_host_handle(
    state: &Arc<std::sync::RwLock<PeerMeshSharedState>>,
    max_wait: Duration,
) -> Option<Arc<P2pTunnelHandle>> {
    let start = tokio::time::Instant::now();
    while start.elapsed() < max_wait {
        if let Ok(st) = state.read() {
            for peer in st.peers.values() {
                if peer.is_host {
                    if let PeerLinkState::Connected(ref h) = peer.state {
                        return Some(h.clone());
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use warlay_core::{UdpRendezvous, UdpRendezvousHandle};

    #[test]
    fn link_offer_serialization_and_magic_prefix() {
        let offer = LinkOffer {
            room_token: Uuid::new_v4(),
            client_id: Uuid::new_v4(),
            p2p_addr: "127.0.0.1:12345".parse().unwrap(),
        };

        let encoded = offer.encode().expect("encode link offer");
        assert!(encoded.starts_with(LINK_OFFER_MAGIC));

        let decoded = LinkOffer::decode(&encoded).expect("decode link offer");
        assert_eq!(decoded.room_token, offer.room_token);
        assert_eq!(decoded.client_id, offer.client_id);
        assert_eq!(decoded.p2p_addr, offer.p2p_addr);

        assert!(LinkOffer::decode(b"INVALID_MAGIC_DATA").is_none());
    }

    #[tokio::test]
    async fn three_nodes_peer_mesh_direct_path_to_host() {
        let rv_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind server");
        let rv_addr = rv_socket.local_addr().expect("server addr");
        let rendezvous = Arc::new(UdpRendezvous::new(rv_socket));
        let _rv_handle: UdpRendezvousHandle = rendezvous.spawn();

        let room_token = Uuid::new_v4();

        let host_id = Uuid::new_v4();
        let j1_id = Uuid::new_v4();
        let j2_id = Uuid::new_v4();

        let host_mesh = PeerMesh::spawn(rv_addr, room_token, host_id, true);
        let j1_mesh = PeerMesh::spawn(rv_addr, room_token, j1_id, false);
        let j2_mesh = PeerMesh::spawn(rv_addr, room_token, j2_id, false);

        let start = tokio::time::Instant::now();
        let timeout_limit = Duration::from_secs(10);

        let mut j1_path = None;
        let mut j2_path = None;

        while start.elapsed() < timeout_limit {
            if j1_path.is_none() {
                j1_path = j1_mesh.best_path_to_host();
            }
            if j2_path.is_none() {
                j2_path = j2_mesh.best_path_to_host();
            }
            if j1_path.is_some() && j2_path.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let j1_path = j1_path.expect("Joiner 1 should obtain a best path to host");
        let j2_path = j2_path.expect("Joiner 2 should obtain a best path to host");

        match j1_path {
            PathToHost::Direct(ref handle) => {
                handle.send.lock().await.write_all(b"j1 to host").await.unwrap();
            }
            other => panic!("expected Direct path for j1, got {:?}", other),
        }

        match j2_path {
            PathToHost::Direct(ref handle) => {
                handle.send.lock().await.write_all(b"j2 to host").await.unwrap();
            }
            other => panic!("expected Direct path for j2, got {:?}", other),
        }

        host_mesh.abort();
        j1_mesh.abort();
        j2_mesh.abort();
    }

    #[tokio::test]
    async fn two_nodes_peer_mesh_datagram_round_trip() {
        let rv_socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind server");
        let rv_addr = rv_socket.local_addr().expect("server addr");
        let rendezvous = Arc::new(UdpRendezvous::new(rv_socket));
        let _rv_handle: UdpRendezvousHandle = rendezvous.spawn();

        let room_token = Uuid::new_v4();

        let node_a_id = Uuid::new_v4();
        let node_b_id = Uuid::new_v4();

        let node_a = PeerMesh::spawn(rv_addr, room_token, node_a_id, true);
        let node_b = PeerMesh::spawn(rv_addr, room_token, node_b_id, false);

        let start = tokio::time::Instant::now();
        let timeout_limit = Duration::from_secs(10);
        let mut connected = false;

        while start.elapsed() < timeout_limit {
            let a_connected = node_a.connected_peers().iter().any(|(id, _)| *id == node_b_id);
            let b_connected = node_b.connected_peers().iter().any(|(id, _)| *id == node_a_id);
            if a_connected && b_connected {
                connected = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        assert!(connected, "Both nodes should be connected to each other");

        let payload_a_to_b = b"datagram_payload_from_a".to_vec();
        let sent_a = node_a.send_datagram_to(node_b_id, payload_a_to_b.clone());
        assert!(sent_a, "send_datagram_to from A to B should return true");

        let payload_b_to_a = b"datagram_payload_from_b".to_vec();
        let sent_b = node_b.send_datagram_to(node_a_id, payload_b_to_a.clone());
        assert!(sent_b, "send_datagram_to from B to A should return true");

        let mut b_received = Vec::new();
        let poll_start = tokio::time::Instant::now();
        while poll_start.elapsed() < Duration::from_secs(5) {
            b_received.extend(node_b.poll_datagrams());
            if b_received
                .iter()
                .any(|(from, data)| *from == node_a_id && data == &payload_a_to_b)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            b_received
                .iter()
                .any(|(from, data)| *from == node_a_id && data == &payload_a_to_b),
            "Node B should receive datagram from Node A with correct peer_id and payload"
        );

        let mut a_received = Vec::new();
        let poll_start = tokio::time::Instant::now();
        while poll_start.elapsed() < Duration::from_secs(5) {
            a_received.extend(node_a.poll_datagrams());
            if a_received
                .iter()
                .any(|(from, data)| *from == node_b_id && data == &payload_b_to_a)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            a_received
                .iter()
                .any(|(from, data)| *from == node_b_id && data == &payload_b_to_a),
            "Node A should receive datagram from Node B with correct peer_id and payload"
        );

        node_a.abort();
        node_b.abort();
    }
}
