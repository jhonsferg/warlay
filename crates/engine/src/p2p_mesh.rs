use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;
use tokio::time::{timeout, Instant};
use tracing::{debug, info, warn};
use uuid::Uuid;

use warlay_protocol::RendezvousMessage;

use crate::p2p_tunnel::{connect_via_relay, punch_and_connect_any, P2pTunnelHandle};

const LINK_OFFER_MAGIC_V1: &[u8] = b"WARLAY_LINK_OFFER_V1";
const LINK_OFFER_MAGIC: &[u8] = b"WARLAY_LINK_OFFER_V2";
const PUNCH_SYNC_MAGIC: &[u8] = b"WARLAY_PUNCH_SYNC_V1";

/// What a peer advertises about the socket it will hole-punch from: its
/// public address as the hub observed it (when known) first, then the same
/// port on each of its LAN addresses, for players behind the same router.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LinkOffer {
    room_token: Uuid,
    client_id: Uuid,
    /// Random per attempt (each attempt punches from a fresh socket), so
    /// both sides can agree on which pair of sockets they're punching; 0 for
    /// older clients, which don't take part in `PunchSync`.
    attempt: u64,
    /// This side gave up on hole punching with the peer and is going
    /// through the helper relay; the peer must do the same.
    relay: bool,
    candidates: Vec<SocketAddr>,
}

/// The single-address offer older clients send.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LinkOfferV1 {
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
        if bytes.len() > LINK_OFFER_MAGIC.len() && &bytes[..LINK_OFFER_MAGIC.len()] == LINK_OFFER_MAGIC {
            return bincode::deserialize(&bytes[LINK_OFFER_MAGIC.len()..]).ok();
        }
        if bytes.len() > LINK_OFFER_MAGIC_V1.len() && &bytes[..LINK_OFFER_MAGIC_V1.len()] == LINK_OFFER_MAGIC_V1 {
            let v1: LinkOfferV1 = bincode::deserialize(&bytes[LINK_OFFER_MAGIC_V1.len()..]).ok()?;
            return Some(Self {
                room_token: v1.room_token,
                client_id: v1.client_id,
                attempt: 0,
                relay: false,
                candidates: vec![v1.p2p_addr],
            });
        }
        None
    }
}

/// Lines up when both sides start punching, the way libp2p's DCUtR does.
/// A NAT that sees the peer's punch arrive *before* its own side sent
/// anything records that inbound packet, and Linux-based NATs (most home
/// routers) then give our outgoing punch a different public port than the
/// one we advertised -- the punch fails. So the initiator measures the
/// round trip with `Ping`/`Pong`, sends `Go` and waits half of it; the
/// responder starts the moment `Go` arrives, and both bursts cross in
/// flight instead of one landing first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum SyncKind {
    Ping,
    /// `ready`: the responder is in the matching attempt and holds the
    /// initiator's candidates for it.
    Pong {
        ready: bool,
    },
    Go,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct PunchSync {
    room_token: Uuid,
    from: Uuid,
    /// The sender's attempt and the receiver's attempt it refers to.
    from_attempt: u64,
    to_attempt: u64,
    nonce: u64,
    kind: SyncKind,
}

impl PunchSync {
    fn encode(&self) -> Option<Vec<u8>> {
        let mut buf = PUNCH_SYNC_MAGIC.to_vec();
        buf.extend(bincode::serialize(self).ok()?);
        Some(buf)
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > PUNCH_SYNC_MAGIC.len() && &bytes[..PUNCH_SYNC_MAGIC.len()] == PUNCH_SYNC_MAGIC {
            return bincode::deserialize(&bytes[PUNCH_SYNC_MAGIC.len()..]).ok();
        }
        None
    }
}

fn random_u64() -> u64 {
    Uuid::new_v4().as_u64_pair().0
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
    /// Every address from the peer's latest `LinkOffer`, punched in parallel.
    pub p2p_candidates: Vec<SocketAddr>,
    /// When that offer arrived: an offer older than one attempt belongs to
    /// a socket the peer has already dropped.
    pub offer_at: Option<Instant>,
    /// The `LinkOffer::attempt` those candidates belong to.
    pub peer_attempt: u64,
    /// The peer's latest offer asked to go through the relay.
    pub peer_wants_relay: bool,
    /// Attempts that failed since the last successful link.
    pub failures: u32,
    pub is_host: bool,
    pub state: PeerLinkState,
}

/// A link counts as usable until QUIC reports it closed (idle timeout, the
/// peer going away, or an explicit close).
fn is_live(handle: &P2pTunnelHandle) -> bool {
    handle.connection.close_reason().is_none()
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
    lan_tx: tokio::sync::mpsc::UnboundedSender<(Uuid, Vec<u8>)>,
    /// The room's helper relay and our ticket for it, if any.
    relay: Option<(SocketAddr, Arc<Vec<u8>>)>,
    /// The attempt currently negotiating with each peer (its own attempt
    /// id), and where the main loop forwards that peer's `PunchSync`s.
    sync_routes: HashMap<Uuid, (u64, SyncSender)>,
}

type SyncSender = tokio::sync::mpsc::UnboundedSender<(PunchSync, Instant)>;

pub struct PeerMeshHandle {
    task: JoinHandle<()>,
    state: Arc<std::sync::RwLock<PeerMeshSharedState>>,
    datagram_rx: std::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<(Uuid, Vec<u8>)>>,
    lan_rx: std::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<(Uuid, Vec<u8>)>>,
    /// Where the mesh registers (rendezvous) and relays, changeable live.
    helper_tx: tokio::sync::watch::Sender<(SocketAddr, Option<SocketAddr>)>,
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
            .filter_map(|p| match p.state {
                PeerLinkState::Connected(ref handle) if is_live(handle) => Some((p.client_id, handle.clone())),
                _ => None,
            })
            .collect()
    }

    pub fn best_path_to_host(&self) -> Option<PathToHost> {
        let state = self.state.read().ok()?;

        // 1. Direct connection to the host?
        for peer in state.peers.values() {
            if peer.is_host {
                if let PeerLinkState::Connected(ref handle) = peer.state {
                    if is_live(handle) {
                        return Some(PathToHost::Direct(handle.clone()));
                    }
                }
            }
        }

        // 2. Via peer (bridge)? Only without a helper relay: with one, the
        // host link itself falls back to the relay, which beats a hop
        // through a peer that may not reach the host yet.
        if state.relay.is_some() {
            return None;
        }
        for peer in state.peers.values() {
            if !peer.is_host {
                if let PeerLinkState::Connected(ref handle) = peer.state {
                    if is_live(handle) {
                        return Some(PathToHost::ViaPeer(peer.client_id, handle.clone()));
                    }
                }
            }
        }

        None
    }

    /// Whether some peer's link is still on its first attempt (or being
    /// re-established after dropping), i.e. a P2P path may show up within
    /// seconds. Peers that already failed keep retrying in the background
    /// but are not worth stalling a game connection for.
    pub fn has_pending_links(&self) -> bool {
        let Ok(st) = self.state.read() else {
            return false;
        };
        st.peers
            .values()
            .any(|p| p.failures == 0 && matches!(p.state, PeerLinkState::NotAttempted | PeerLinkState::Connecting))
    }

    /// Sends a voice frame to `peer_id` as an unreliable datagram.
    pub fn send_datagram_to(&self, peer_id: Uuid, data: Vec<u8>) -> bool {
        self.send_tagged(peer_id, DATAGRAM_VOICE, &data)
    }

    fn send_tagged(&self, peer_id: Uuid, tag: u8, data: &[u8]) -> bool {
        let Ok(st) = self.state.read() else {
            return false;
        };
        if let Some(peer) = st.peers.get(&peer_id) {
            if let PeerLinkState::Connected(ref handle) = peer.state {
                let mut framed = Vec::with_capacity(data.len() + 1);
                framed.push(tag);
                framed.extend_from_slice(data);
                return handle.connection.send_datagram(framed.into()).is_ok();
            }
        }
        false
    }

    /// Sends a captured LAN broadcast (the game's announcements, searches)
    /// to every connected peer, so each one replays it on its own network.
    /// Returns how many peers it went to.
    pub fn broadcast_lan_packet(&self, data: &[u8]) -> usize {
        self.connected_peers()
            .iter()
            .filter(|(peer_id, _)| self.send_tagged(*peer_id, DATAGRAM_LAN, data))
            .count()
    }

    /// Voice frames received since the last call.
    pub fn poll_datagrams(&self) -> Vec<(Uuid, Vec<u8>)> {
        drain(&self.datagram_rx)
    }

    /// LAN broadcasts received from peers since the last call.
    pub fn poll_lan_packets(&self) -> Vec<(Uuid, Vec<u8>)> {
        drain(&self.lan_rx)
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
                    PeerLinkState::Connected(ref h) if h.relayed => "relayed",
                    PeerLinkState::Connected(_) => "connected",
                    PeerLinkState::Failed => "failed",
                };
                (p.client_id, p.is_host, p.p2p_addr, tag)
            })
            .collect()
    }

    /// Moves the mesh to another helper (the room's old one went away):
    /// registers at the new rendezvous and relays through the new relay.
    /// The room ticket stays valid -- every helper shares the main server's
    /// secret -- and links that are already up keep running untouched.
    pub fn switch_helper(&self, rendezvous_addr: SocketAddr, relay_addr: Option<SocketAddr>) {
        if let Ok(mut st) = self.state.write() {
            st.relay = match (relay_addr, st.relay.take()) {
                (Some(addr), Some((_, ticket))) => Some((addr, ticket)),
                (_, previous) => previous,
            };
        }
        let _ = self.helper_tx.send((rendezvous_addr, relay_addr));
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

/// First byte of every mesh datagram, saying what it carries.
const DATAGRAM_VOICE: u8 = 0x01;
const DATAGRAM_LAN: u8 = 0x02;

fn drain(rx: &std::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<(Uuid, Vec<u8>)>>) -> Vec<(Uuid, Vec<u8>)> {
    let mut items = Vec::new();
    if let Ok(mut rx) = rx.lock() {
        while let Ok(item) = rx.try_recv() {
            items.push(item);
        }
    }
    items
}

/// How a mesh joins its room.
#[derive(Debug, Clone)]
pub struct MeshConfig {
    pub rendezvous_addr: SocketAddr,
    pub room_token: Uuid,
    pub own_client_id: Uuid,
    pub is_host: bool,
    /// The room ticket signed by the main server. Helpers only admit
    /// ticket holders; `None` registers the old, unauthenticated way.
    pub ticket: Option<Vec<u8>>,
    /// The helper's relay, used only when hole punching keeps failing.
    pub relay_addr: Option<SocketAddr>,
}

/// After this many failed punch attempts with a peer, go through the relay.
const RELAY_AFTER_FAILURES: u32 = 2;

pub struct PeerMesh;

impl PeerMesh {
    pub fn spawn(rendezvous_addr: SocketAddr, room_token: Uuid, own_client_id: Uuid, is_host: bool) -> PeerMeshHandle {
        Self::spawn_with(MeshConfig {
            rendezvous_addr,
            room_token,
            own_client_id,
            is_host,
            ticket: None,
            relay_addr: None,
        })
    }

    pub fn spawn_with(config: MeshConfig) -> PeerMeshHandle {
        let MeshConfig {
            rendezvous_addr,
            room_token,
            own_client_id,
            is_host,
            ticket,
            relay_addr,
        } = config;
        let relay = match (relay_addr, &ticket) {
            (Some(addr), Some(ticket)) => Some((addr, Arc::new(ticket.clone()))),
            _ => None,
        };
        let (datagram_tx, datagram_rx) = tokio::sync::mpsc::unbounded_channel();
        let (lan_tx, lan_rx) = tokio::sync::mpsc::unbounded_channel();
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
            lan_tx,
            relay,
            sync_routes: HashMap::new(),
        }));

        let state_task = state.clone();
        let (helper_tx, helper_rx) = tokio::sync::watch::channel((rendezvous_addr, relay_addr));
        let task = tokio::spawn(async move {
            run_peer_mesh(
                rendezvous_addr,
                room_token,
                own_client_id,
                is_host,
                ticket,
                helper_rx,
                state_task,
            )
            .await;
        });

        PeerMeshHandle {
            task,
            state,
            datagram_rx: std::sync::Mutex::new(datagram_rx),
            lan_rx: std::sync::Mutex::new(lan_rx),
            helper_tx,
        }
    }
}

impl PeerEntry {
    fn new(client_id: Uuid, addr: SocketAddr, is_host: bool) -> Self {
        Self {
            client_id,
            addr,
            p2p_addr: None,
            p2p_candidates: Vec::new(),
            offer_at: None,
            peer_attempt: 0,
            peer_wants_relay: false,
            failures: 0,
            is_host,
            state: PeerLinkState::NotAttempted,
        }
    }
}

/// How long one attempt keeps offering and waiting for the peer's offer.
const OFFER_WINDOW: Duration = Duration::from_millis(2500);

async fn run_peer_mesh(
    rendezvous_addr: SocketAddr,
    room_token: Uuid,
    own_client_id: Uuid,
    is_host: bool,
    ticket: Option<Vec<u8>>,
    mut helper_rx: tokio::sync::watch::Receiver<(SocketAddr, Option<SocketAddr>)>,
    state: Arc<std::sync::RwLock<PeerMeshSharedState>>,
) {
    let mut rendezvous_addr = rendezvous_addr;
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

    let register = match ticket {
        Some(ticket) => RendezvousMessage::RegisterWithTicket { ticket },
        None => RendezvousMessage::Register {
            room_token,
            client_id: own_client_id,
            is_host,
        },
    };
    let reg_msg = match register.encode() {
        Ok(b) => b,
        Err(err) => {
            warn!(%err, "failed to encode Register message in PeerMesh");
            return;
        }
    };

    let mut buf = [0_u8; 2048];
    let mut registered = false;

    while !registered {
        // The helper may be replaced before it ever answered.
        if helper_rx.has_changed().unwrap_or(false) {
            rendezvous_addr = helper_rx.borrow_and_update().0;
        }
        let _ = socket.send_to(&reg_msg, rendezvous_addr).await;
        match timeout(Duration::from_millis(300), socket.recv_from(&mut buf)).await {
            Ok(Ok((len, src))) if src == rendezvous_addr => {
                if let Ok(RendezvousMessage::PeerList { peers }) = RendezvousMessage::decode(&buf[..len]) {
                    registered = true;
                    let mut st = state.write().unwrap();
                    for p in peers {
                        if p.client_id != own_client_id {
                            st.peers
                                .entry(p.client_id)
                                .or_insert_with(|| PeerEntry::new(p.client_id, p.addr, p.is_host));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    info!(%room_token, %own_client_id, is_host, "registered presence with rendezvous server in PeerMesh");
    let socket = Arc::new(socket);
    let mut ctx = AttemptContext {
        rendezvous_addr,
        room_token,
        own_client_id,
        is_host,
    };

    trigger_connection_attempts(ctx, &state, &socket);

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

    let mut register_retry = tokio::time::interval(Duration::from_millis(700));
    register_retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            // The room moved to another helper (the old one died): register
            // there. Established links don't depend on the helper and are
            // left alone; only new or broken ones go through the new one.
            Ok(()) = helper_rx.changed() => {
                let next = helper_rx.borrow_and_update().0;
                if next != rendezvous_addr {
                    info!(%room_token, old = %rendezvous_addr, new = %next, "room moved to another helper");
                    rendezvous_addr = next;
                    ctx.rendezvous_addr = next;
                    registered = false;
                    let _ = socket.send_to(&reg_msg, rendezvous_addr).await;
                }
            }
            _ = register_retry.tick(), if !registered => {
                let _ = socket.send_to(&reg_msg, rendezvous_addr).await;
            }
            _ = heartbeat_interval.tick() => {
                let _ = socket.send_to(&heartbeat_msg, rendezvous_addr).await;
            }
            _ = attempt_interval.tick() => {
                trigger_connection_attempts(ctx, &state, &socket);
            }
            recv_res = socket.recv_from(&mut buf) => {
                match recv_res {
                    Ok((len, src)) if src == rendezvous_addr => {
                        if let Ok(msg) = RendezvousMessage::decode(&buf[..len]) {
                            match msg {
                                RendezvousMessage::PeerList { peers } => {
                                    registered = true;
                                    let mut st = state.write().unwrap();
                                    for p in peers {
                                        if p.client_id != own_client_id {
                                            st.peers
                                                .entry(p.client_id)
                                                .or_insert_with(|| PeerEntry::new(p.client_id, p.addr, p.is_host));
                                        }
                                    }
                                    drop(st);
                                    trigger_connection_attempts(ctx, &state, &socket);
                                }
                                RendezvousMessage::PeerJoined { peer } => {
                                    if peer.client_id != own_client_id {
                                        info!(%room_token, peer_id = %peer.client_id, peer_addr = %peer.addr, "PeerJoined notification received");
                                        let mut st = state.write().unwrap();
                                        st.peers.entry(peer.client_id)
                                            .and_modify(|e| {
                                                e.addr = peer.addr;
                                                e.is_host = peer.is_host;
                                            })
                                            .or_insert_with(|| PeerEntry::new(peer.client_id, peer.addr, peer.is_host));
                                        drop(st);
                                        trigger_connection_attempts(ctx, &state, &socket);
                                    }
                                }
                                // The helper restarted (or our public address
                                // changed) and no longer knows us: register again
                                // so newcomers can still find us. Links already
                                // up don't depend on it and keep running.
                                RendezvousMessage::RegisterAgain => {
                                    if registered {
                                        info!(%room_token, "rendezvous lost our registration, registering again");
                                        registered = false;
                                        let _ = socket.send_to(&reg_msg, rendezvous_addr).await;
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
                                RendezvousMessage::Signal { room_token: signal_room, to, payload, .. }
                                    if signal_room == room_token && to == own_client_id =>
                                {
                                    if let Some(offer) = LinkOffer::decode(&payload) {
                                        accept_offer(ctx, &state, &socket, offer, None);
                                    } else if let Some(sync) = PunchSync::decode(&payload) {
                                        handle_sync(ctx, &state, &socket, sync, None);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Ok((len, src)) => {
                        if let Some(offer) = LinkOffer::decode(&buf[..len]) {
                            accept_offer(ctx, &state, &socket, offer, Some(src));
                        } else if let Some(sync) = PunchSync::decode(&buf[..len]) {
                            handle_sync(ctx, &state, &socket, sync, Some(src));
                        }
                    }
                    Err(_) => {}
                }
            }
        }
    }
}

/// Records a peer's `LinkOffer`, received either straight from its
/// registration socket (`direct_src`) or relayed by the hub as a `Signal`.
fn accept_offer(
    ctx: AttemptContext,
    state: &Arc<std::sync::RwLock<PeerMeshSharedState>>,
    reg_socket: &Arc<UdpSocket>,
    offer: LinkOffer,
    direct_src: Option<SocketAddr>,
) {
    if offer.room_token != ctx.room_token || offer.client_id == ctx.own_client_id {
        return;
    }
    // An unspecified IP means "the address you received this from" (older
    // clients send their bind address, 0.0.0.0:port); meaningless when the
    // hub relayed the offer.
    let candidates: Vec<SocketAddr> = offer
        .candidates
        .iter()
        .filter_map(|c| match (c.ip().is_unspecified(), direct_src) {
            (false, _) => Some(*c),
            (true, Some(src)) => Some(SocketAddr::new(src.ip(), c.port())),
            (true, None) => None,
        })
        .collect();
    if candidates.is_empty() {
        return;
    }
    debug!(room_token = %ctx.room_token, peer_id = %offer.client_id, ?candidates, relayed = direct_src.is_none(), "received LinkOffer from peer");
    {
        let mut st = state.write().unwrap();
        let entry = match (st.peers.contains_key(&offer.client_id), direct_src) {
            (true, _) => st.peers.get_mut(&offer.client_id).unwrap(),
            (false, Some(src)) => st
                .peers
                .entry(offer.client_id)
                .or_insert_with(|| PeerEntry::new(offer.client_id, src, false)),
            // Not announced by the hub yet and no address to answer on.
            (false, None) => return,
        };
        if entry.peer_attempt == offer.attempt
            && entry.p2p_candidates == candidates
            && entry.offer_at.is_some_and(|at| at.elapsed() < OFFER_WINDOW)
        {
            // The same offer arriving down both paths.
            entry.offer_at = Some(Instant::now());
            return;
        }
        entry.p2p_addr = candidates.first().copied();
        entry.p2p_candidates = candidates;
        entry.peer_attempt = offer.attempt;
        entry.peer_wants_relay = offer.relay;
        entry.offer_at = Some(Instant::now());
    }
    trigger_connection_attempts(ctx, state, reg_socket);
}

/// Answers a `Ping` right here in the receive loop (its turnaround time is
/// part of the measured round trip) and hands `Pong`/`Go` to the attempt
/// they belong to.
fn handle_sync(
    ctx: AttemptContext,
    state: &Arc<std::sync::RwLock<PeerMeshSharedState>>,
    reg_socket: &Arc<UdpSocket>,
    sync: PunchSync,
    direct_src: Option<SocketAddr>,
) {
    if sync.room_token != ctx.room_token || sync.from == ctx.own_client_id {
        return;
    }
    let received_at = Instant::now();
    let st = state.read().unwrap();
    let in_attempt = st
        .sync_routes
        .get(&sync.from)
        .filter(|(own_attempt, _)| *own_attempt == sync.to_attempt);

    if sync.kind != SyncKind::Ping {
        if let Some((_, tx)) = in_attempt {
            let _ = tx.send((sync, received_at));
        }
        return;
    }

    let ready = in_attempt.is_some()
        && st
            .peers
            .get(&sync.from)
            .is_some_and(|p| p.peer_attempt == sync.from_attempt && !p.p2p_candidates.is_empty());
    drop(st);
    let pong = PunchSync {
        room_token: ctx.room_token,
        from: ctx.own_client_id,
        from_attempt: sync.to_attempt,
        to_attempt: sync.from_attempt,
        nonce: sync.nonce,
        kind: SyncKind::Pong { ready },
    };
    let Some(pong_bytes) = pong.encode() else {
        return;
    };
    // Back the way the ping came, so both legs of the measurement match.
    match direct_src {
        Some(src) => {
            let _ = reg_socket.try_send_to(&pong_bytes, src);
        }
        None => {
            let signal = RendezvousMessage::Signal {
                room_token: ctx.room_token,
                from: ctx.own_client_id,
                to: sync.from,
                payload: pong_bytes,
            };
            if let Ok(bytes) = signal.encode() {
                let _ = reg_socket.try_send_to(&bytes, ctx.rendezvous_addr);
            }
        }
    }
}

/// Sends a `PunchSync` straight to the peer's registration socket and,
/// in case that path is blocked, through the hub.
async fn send_sync(
    ctx: AttemptContext,
    reg_socket: &UdpSocket,
    peer_id: Uuid,
    peer_reg_addr: SocketAddr,
    sync: PunchSync,
) {
    let Some(bytes) = sync.encode() else {
        return;
    };
    let _ = reg_socket.send_to(&bytes, peer_reg_addr).await;
    let signal = RendezvousMessage::Signal {
        room_token: ctx.room_token,
        from: ctx.own_client_id,
        to: peer_id,
        payload: bytes,
    };
    if let Ok(bytes) = signal.encode() {
        let _ = reg_socket.send_to(&bytes, ctx.rendezvous_addr).await;
    }
}

/// How long either side spends lining up the punch before going ahead
/// unsynchronized.
const SYNC_WINDOW: Duration = Duration::from_millis(2000);

/// Initiator side of `PunchSync`. Returns once both sides are about to
/// punch at the same moment, or after `SYNC_WINDOW` without an answer.
#[allow(clippy::too_many_arguments)]
async fn sync_as_initiator(
    ctx: AttemptContext,
    reg_socket: &UdpSocket,
    peer_id: Uuid,
    peer_reg_addr: SocketAddr,
    own_attempt: u64,
    state: &Arc<std::sync::RwLock<PeerMeshSharedState>>,
    target: &mut (u64, Vec<SocketAddr>),
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<(PunchSync, Instant)>,
) -> bool {
    let deadline = Instant::now() + SYNC_WINDOW;
    while Instant::now() < deadline {
        // The peer may have moved on to a newer attempt (new socket).
        if let Some(p) = state.read().unwrap().peers.get(&peer_id) {
            if p.peer_attempt != 0 && !p.p2p_candidates.is_empty() {
                *target = (p.peer_attempt, p.p2p_candidates.clone());
            }
        }
        let nonce = random_u64();
        let ping = PunchSync {
            room_token: ctx.room_token,
            from: ctx.own_client_id,
            from_attempt: own_attempt,
            to_attempt: target.0,
            nonce,
            kind: SyncKind::Ping,
        };
        let sent_at = Instant::now();
        send_sync(ctx, reg_socket, peer_id, peer_reg_addr, ping).await;

        let wait_until = (sent_at + Duration::from_millis(250)).min(deadline);
        while let Ok(Some((msg, at))) = tokio::time::timeout_at(wait_until, rx.recv()).await {
            if msg.nonce != nonce {
                continue;
            }
            match msg.kind {
                SyncKind::Pong { ready: true } => {
                    let rtt = at.saturating_duration_since(sent_at);
                    let go = PunchSync {
                        kind: SyncKind::Go,
                        ..ping
                    };
                    send_sync(ctx, reg_socket, peer_id, peer_reg_addr, go).await;
                    debug!(room_token = %ctx.room_token, %peer_id, ?rtt, "punch synchronized");
                    tokio::time::sleep(rtt / 2).await;
                    return true;
                }
                SyncKind::Pong { ready: false } => {
                    // The peer hasn't caught up with our offer yet.
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    break;
                }
                _ => {}
            }
        }
    }
    false
}

/// Responder side of `PunchSync`: waits for the initiator's `Go`.
async fn sync_as_responder(rx: &mut tokio::sync::mpsc::UnboundedReceiver<(PunchSync, Instant)>) -> Option<PunchSync> {
    let deadline = Instant::now() + SYNC_WINDOW + Duration::from_millis(500);
    while let Ok(Some((msg, _))) = tokio::time::timeout_at(deadline, rx.recv()).await {
        if msg.kind == SyncKind::Go {
            return Some(msg);
        }
    }
    None
}

#[derive(Clone, Copy)]
struct AttemptContext {
    rendezvous_addr: SocketAddr,
    room_token: Uuid,
    own_client_id: Uuid,
    is_host: bool,
}

/// Asks the hub which public address `socket` maps to. `None` if the hub
/// doesn't answer (an older server without `Probe` support, or loss).
async fn probe_public_addr(socket: &UdpSocket, rendezvous_addr: SocketAddr) -> Option<SocketAddr> {
    let probe = RendezvousMessage::Probe.encode().ok()?;
    let mut buf = [0_u8; 512];
    for _ in 0..3 {
        let _ = socket.send_to(&probe, rendezvous_addr).await;
        let deadline = Instant::now() + Duration::from_millis(300);
        while let Ok(Ok((len, src))) = tokio::time::timeout_at(deadline, socket.recv_from(&mut buf)).await {
            if src != rendezvous_addr {
                continue;
            }
            if let Ok(RendezvousMessage::ProbeReply { observed }) = RendezvousMessage::decode(&buf[..len]) {
                return Some(observed);
            }
        }
    }
    None
}

/// The addresses a peer can try for `local_port`: the public mapping first,
/// then each LAN address (two players behind the same router usually can't
/// reach each other through its public address).
fn link_candidates(public: Option<SocketAddr>, local_port: u16) -> Vec<SocketAddr> {
    let mut candidates: Vec<SocketAddr> = public.into_iter().collect();
    for ip in crate::subnet::local_ipv4_addrs() {
        let addr = SocketAddr::new(ip.into(), local_port);
        if !candidates.contains(&addr) {
            candidates.push(addr);
        }
    }
    candidates
}

fn clear_sync_route(state: &Arc<std::sync::RwLock<PeerMeshSharedState>>, peer_id: Uuid, own_attempt: u64) {
    let mut st = state.write().unwrap();
    if st
        .sync_routes
        .get(&peer_id)
        .is_some_and(|(attempt, _)| *attempt == own_attempt)
    {
        st.sync_routes.remove(&peer_id);
    }
}

fn mark_failed(state: &Arc<std::sync::RwLock<PeerMeshSharedState>>, peer_id: Uuid) {
    let mut st = state.write().unwrap();
    if let Some(p) = st.peers.get_mut(&peer_id) {
        p.state = PeerLinkState::Failed;
        p.failures += 1;
    }
}

fn trigger_connection_attempts(
    ctx: AttemptContext,
    state: &Arc<std::sync::RwLock<PeerMeshSharedState>>,
    reg_socket: &Arc<UdpSocket>,
) {
    let AttemptContext {
        rendezvous_addr,
        room_token,
        own_client_id,
        is_host,
    } = ctx;
    let mut targets = Vec::new();
    {
        let mut st = state.write().unwrap();
        for peer in st.peers.values_mut() {
            match peer.state {
                PeerLinkState::NotAttempted | PeerLinkState::Failed => {
                    peer.state = PeerLinkState::Connecting;
                    // Keep an offer that arrived just before this attempt
                    // started -- the peer is punching from that socket right
                    // now; only drop offers from its previous attempts.
                    let fresh = peer.offer_at.is_some_and(|at| at.elapsed() < OFFER_WINDOW);
                    if !fresh {
                        peer.p2p_addr = None;
                        peer.p2p_candidates.clear();
                        peer.offer_at = None;
                    }
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
            let Ok(local_socket) = UdpSocket::bind("0.0.0.0:0").await else {
                mark_failed(&state_clone, peer_id);
                return;
            };
            let Ok(local_p2p_addr) = local_socket.local_addr() else {
                mark_failed(&state_clone, peer_id);
                return;
            };

            // Registered before offering, so the peer's sync messages for this
            // attempt find their way here from the receive loop.
            let own_attempt = random_u64();
            let (sync_tx, mut sync_rx) = tokio::sync::mpsc::unbounded_channel();
            state_clone
                .write()
                .unwrap()
                .sync_routes
                .insert(peer_id, (own_attempt, sync_tx));

            let (relay_cfg, own_failures) = {
                let st = state_clone.read().unwrap();
                (st.relay.clone(), st.peers.get(&peer_id).map_or(0, |p| p.failures))
            };
            let mut use_relay = relay_cfg.is_some() && own_failures >= RELAY_AFTER_FAILURES;

            let public_addr = probe_public_addr(&local_socket, rendezvous_addr).await;
            let offer = LinkOffer {
                room_token,
                client_id: own_client_id,
                attempt: own_attempt,
                relay: use_relay,
                candidates: link_candidates(public_addr, local_p2p_addr.port()),
            };
            debug!(%room_token, %peer_id, candidates = ?offer.candidates, "offering P2P link candidates");

            let Ok(offer_bytes) = offer.encode() else {
                clear_sync_route(&state_clone, peer_id, own_attempt);
                mark_failed(&state_clone, peer_id);
                return;
            };

            // The same offer also goes through the hub: the direct path needs
            // each side's NAT to let the other's registration socket in, and
            // two players behind one router can't use it at all unless the
            // router supports hairpinning.
            let signal_bytes = RendezvousMessage::Signal {
                room_token,
                from: own_client_id,
                to: peer_id,
                payload: offer_bytes.clone(),
            }
            .encode()
            .ok();

            let offer_deadline = Instant::now() + OFFER_WINDOW;
            let mut target_candidates = Vec::new();
            let mut target_attempt = 0;
            let mut round = 0_u32;

            while Instant::now() < offer_deadline {
                let _ = reg_socket_clone.send_to(&offer_bytes, peer_reg_addr).await;
                if round.is_multiple_of(5) {
                    if let Some(signal) = &signal_bytes {
                        let _ = reg_socket_clone.send_to(signal, rendezvous_addr).await;
                    }
                }
                round += 1;

                {
                    let st = state_clone.read().unwrap();
                    if let Some(peer) = st.peers.get(&peer_id) {
                        if !peer.p2p_candidates.is_empty() {
                            target_candidates = peer.p2p_candidates.clone();
                            target_attempt = peer.peer_attempt;
                            use_relay |= relay_cfg.is_some() && peer.peer_wants_relay;
                            break;
                        }
                    }
                }

                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            for _ in 0..3 {
                let _ = reg_socket_clone.send_to(&offer_bytes, peer_reg_addr).await;
            }
            if let Some(signal) = &signal_bytes {
                let _ = reg_socket_clone.send_to(signal, rendezvous_addr).await;
            }

            if target_candidates.is_empty() {
                debug!(%room_token, %peer_id, %peer_reg_addr, "timed out waiting for LinkOffer from peer");
                clear_sync_route(&state_clone, peer_id, own_attempt);
                mark_failed(&state_clone, peer_id);
                return;
            }

            let act_as_quic_server = if is_host != peer_is_host {
                is_host
            } else {
                own_client_id < peer_id
            };

            // Older peers (attempt 0) don't speak PunchSync; relayed links
            // don't punch at all.
            if target_attempt != 0 && !use_relay {
                if act_as_quic_server {
                    let mut target = (target_attempt, target_candidates.clone());
                    let synced = sync_as_initiator(
                        ctx,
                        &reg_socket_clone,
                        peer_id,
                        peer_reg_addr,
                        own_attempt,
                        &state_clone,
                        &mut target,
                        &mut sync_rx,
                    )
                    .await;
                    target_candidates = target.1;
                    if !synced {
                        debug!(%room_token, %peer_id, "no answer to punch sync, punching unsynchronized");
                    }
                } else if let Some(go) = sync_as_responder(&mut sync_rx).await {
                    // Punch exactly the socket the initiator is punching from.
                    let st = state_clone.read().unwrap();
                    if let Some(p) = st.peers.get(&peer_id) {
                        if p.peer_attempt == go.from_attempt && !p.p2p_candidates.is_empty() {
                            target_candidates = p.p2p_candidates.clone();
                        }
                    }
                } else {
                    debug!(%room_token, %peer_id, "no punch sync from peer, punching unsynchronized");
                }
            }
            clear_sync_route(&state_clone, peer_id, own_attempt);

            let res = match (&relay_cfg, use_relay) {
                (Some((relay_addr, ticket)), true) => {
                    info!(%room_token, %peer_id, %relay_addr, "hole punching keeps failing, linking through the helper relay");
                    timeout(
                        Duration::from_secs(15),
                        connect_via_relay(*relay_addr, ticket, peer_id, room_token, act_as_quic_server),
                    )
                    .await
                }
                _ => {
                    timeout(
                        Duration::from_secs(4),
                        punch_and_connect_any(local_socket, &target_candidates, room_token, act_as_quic_server),
                    )
                    .await
                }
            };

            let mut st = state_clone.write().unwrap();
            match res {
                Ok(Ok(handle)) => {
                    let remote = handle.connection.remote_address();
                    info!(%room_token, %peer_id, %remote, "successfully connected P2P link to peer");
                    let handle_arc = Arc::new(handle);
                    if let Some(p) = st.peers.get_mut(&peer_id) {
                        p.state = PeerLinkState::Connected(handle_arc.clone());
                        p.failures = 0;
                        p.p2p_addr = Some(remote);
                    }

                    let watch_state = state_clone.clone();
                    let watch_handle = handle_arc.clone();
                    tokio::spawn(async move {
                        watch_link(watch_state, peer_id, watch_handle).await;
                    });

                    if st.datagram_peers.insert(peer_id) {
                        let handle_task = handle_arc.clone();
                        let tx = st.datagram_tx.clone();
                        let lan_tx = st.lan_tx.clone();
                        let datagram_task = tokio::spawn(async move {
                            run_datagram_listener_for_peer(peer_id, handle_task, tx, lan_tx).await;
                        });
                        st.datagram_tasks.push(datagram_task);
                    }

                    // A joiner relays streams that *other* joiners open on
                    // their link to it (`PathToHost::ViaPeer`) on to the host.
                    if !is_host && !peer_is_host && st.bridged_peers.insert(peer_id) {
                        let state_task = state_clone.clone();
                        let bridge_task = tokio::spawn(async move {
                            run_bridge_for_peer(state_task, peer_id, handle_arc).await;
                        });
                        st.bridge_tasks.push(bridge_task);
                    }
                }
                _ => {
                    debug!(%room_token, %peer_id, ?target_candidates, "P2P connection attempt to peer failed or timed out");
                    if let Some(p) = st.peers.get_mut(&peer_id) {
                        p.state = PeerLinkState::Failed;
                        p.failures += 1;
                    }
                }
            }
        });
    }
}

/// Waits for a connected link to close and hands the peer back to the
/// retry loop, so a dropped link gets re-punched instead of being reported
/// as connected forever.
async fn watch_link(state: Arc<std::sync::RwLock<PeerMeshSharedState>>, peer_id: Uuid, handle: Arc<P2pTunnelHandle>) {
    let reason = handle.connection.closed().await;
    warn!(%peer_id, %reason, "P2P link to peer closed, will retry");
    let mut st = state.write().unwrap();
    let current = matches!(
        st.peers.get(&peer_id).map(|p| &p.state),
        Some(PeerLinkState::Connected(h)) if Arc::ptr_eq(h, &handle)
    );
    if current {
        if let Some(p) = st.peers.get_mut(&peer_id) {
            p.state = PeerLinkState::Failed;
        }
        st.datagram_peers.remove(&peer_id);
        st.bridged_peers.remove(&peer_id);
    }
}

/// Runs on a joiner for each link to another joiner: every bidirectional
/// stream that joiner opens (one per game connection, tagged `0x01`) is
/// spliced onto a fresh stream to the host over our own direct link.
async fn run_bridge_for_peer(
    state: Arc<std::sync::RwLock<PeerMeshSharedState>>,
    peer_id: Uuid,
    handle_peer: Arc<P2pTunnelHandle>,
) {
    loop {
        let (mut peer_send, mut peer_recv) = match handle_peer.connection.accept_bi().await {
            Ok(streams) => streams,
            Err(_) => return,
        };
        let state = state.clone();
        tokio::spawn(async move {
            let mut tag = [0_u8; 1];
            if peer_recv.read_exact(&mut tag).await.is_err() || tag[0] != 0x01 {
                return;
            }
            debug!(%peer_id, "bridge stream opened by peer, connecting to host");

            let Some(host_handle) = wait_for_host_handle(&state, Duration::from_secs(5)).await else {
                warn!(%peer_id, "bridge has no link to the host, refusing stream");
                let _ = peer_send.reset(0u32.into());
                return;
            };
            let (mut host_send, mut host_recv) = match host_handle.connection.open_bi().await {
                Ok(s) => s,
                Err(err) => {
                    warn!(%err, %peer_id, "failed to open bi stream to host in bridge task");
                    let _ = peer_send.reset(0u32.into());
                    return;
                }
            };
            if host_send.write_all(&[0x01]).await.is_err() {
                return;
            }

            let peer_to_host = async {
                let _ = tokio::io::copy(&mut peer_recv, &mut host_send).await;
                let _ = host_send.finish();
            };
            let host_to_peer = async {
                let _ = tokio::io::copy(&mut host_recv, &mut peer_send).await;
                let _ = peer_send.finish();
            };
            tokio::join!(peer_to_host, host_to_peer);
        });
    }
}

async fn run_datagram_listener_for_peer(
    peer_id: Uuid,
    handle_peer: Arc<P2pTunnelHandle>,
    voice_tx: tokio::sync::mpsc::UnboundedSender<(Uuid, Vec<u8>)>,
    lan_tx: tokio::sync::mpsc::UnboundedSender<(Uuid, Vec<u8>)>,
) {
    loop {
        match handle_peer.connection.read_datagram().await {
            Ok(bytes) => {
                let sent = match bytes.split_first() {
                    Some((&DATAGRAM_LAN, payload)) => lan_tx.send((peer_id, payload.to_vec())).is_ok(),
                    Some((&DATAGRAM_VOICE, payload)) => voice_tx.send((peer_id, payload.to_vec())).is_ok(),
                    _ => true,
                };
                if !sent {
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
            attempt: 42,
            relay: true,
            candidates: vec![
                "203.0.113.7:50123".parse().unwrap(),
                "192.168.1.20:12345".parse().unwrap(),
            ],
        };

        let encoded = offer.encode().expect("encode link offer");
        assert!(encoded.starts_with(LINK_OFFER_MAGIC));

        let decoded = LinkOffer::decode(&encoded).expect("decode link offer");
        assert_eq!(decoded.room_token, offer.room_token);
        assert_eq!(decoded.client_id, offer.client_id);
        assert_eq!(decoded.candidates, offer.candidates);

        assert!(LinkOffer::decode(b"INVALID_MAGIC_DATA").is_none());
    }

    #[test]
    fn link_offer_from_older_clients_still_decodes() {
        let v1 = LinkOfferV1 {
            room_token: Uuid::new_v4(),
            client_id: Uuid::new_v4(),
            p2p_addr: "0.0.0.0:12345".parse().unwrap(),
        };
        let mut encoded = LINK_OFFER_MAGIC_V1.to_vec();
        encoded.extend(bincode::serialize(&v1).unwrap());

        let decoded = LinkOffer::decode(&encoded).expect("decode v1 link offer");
        assert_eq!(decoded.client_id, v1.client_id);
        assert_eq!(decoded.candidates, vec![v1.p2p_addr]);
    }

    /// The room's helper dies: the link already up survives untouched, and
    /// after switching to a new helper a newcomer links up through it.
    #[tokio::test]
    async fn room_survives_its_helper_and_moves_to_another() {
        let secret = b"failover-secret".to_vec();
        let helper = |secret: Vec<u8>| async move {
            let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let addr = socket.local_addr().unwrap();
            let handle = Arc::new(UdpRendezvous::new(socket).with_ticket_secret(secret)).spawn();
            (addr, handle)
        };
        let (old_addr, old_helper) = helper(secret.clone()).await;
        let (new_addr, _new_helper) = helper(secret.clone()).await;

        let room = Uuid::new_v4();
        let mesh_for = |user: Uuid, is_host: bool, rendezvous: SocketAddr| {
            let t = warlay_protocol::RoomTicket {
                room_id: room,
                user_id: user,
                is_host,
                expires_unix: u64::MAX,
            };
            PeerMesh::spawn_with(MeshConfig {
                rendezvous_addr: rendezvous,
                room_token: room,
                own_client_id: user,
                is_host,
                ticket: Some(warlay_crypto::sign_token(&secret, &t.encode().unwrap())),
                relay_addr: None,
            })
        };
        let (host_id, joiner_id, late_id) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let host = mesh_for(host_id, true, old_addr);
        let joiner = mesh_for(joiner_id, false, old_addr);
        let start = tokio::time::Instant::now();
        while joiner.best_path_to_host().is_none() {
            assert!(start.elapsed() < Duration::from_secs(10), "never linked");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let Some(PathToHost::Direct(link_before)) = joiner.best_path_to_host() else {
            panic!("expected a direct link");
        };

        // The helper dies; everyone is told to use the new one.
        old_helper.task.abort();
        host.switch_helper(new_addr, None);
        joiner.switch_helper(new_addr, None);
        let late = mesh_for(late_id, false, new_addr);

        let start = tokio::time::Instant::now();
        while late.best_path_to_host().is_none() || !host.connected_peers().iter().any(|(id, _)| *id == late_id) {
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "newcomer never linked via the new helper"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        // The old link is the very same connection, still alive.
        let Some(PathToHost::Direct(link_after)) = joiner.best_path_to_host() else {
            panic!("joiner lost its link");
        };
        assert!(Arc::ptr_eq(&link_before, &link_after));
        assert!(is_live(&link_after));

        host.abort();
        joiner.abort();
        late.abort();
    }

    #[tokio::test]
    async fn ticket_holders_link_up_and_exchange_lan_and_voice_datagrams() {
        let secret = b"mesh-ticket-secret".to_vec();
        let rv_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let rv_addr = rv_socket.local_addr().unwrap();
        Arc::new(UdpRendezvous::new(rv_socket).with_ticket_secret(secret.clone())).spawn();

        let room = Uuid::new_v4();
        let mesh_for = |user: Uuid, is_host: bool| {
            let t = warlay_protocol::RoomTicket {
                room_id: room,
                user_id: user,
                is_host,
                expires_unix: u64::MAX,
            };
            PeerMesh::spawn_with(MeshConfig {
                rendezvous_addr: rv_addr,
                room_token: room,
                own_client_id: user,
                is_host,
                ticket: Some(warlay_crypto::sign_token(&secret, &t.encode().unwrap())),
                relay_addr: None,
            })
        };
        let (host_id, joiner_id) = (Uuid::new_v4(), Uuid::new_v4());
        let host = mesh_for(host_id, true);
        let joiner = mesh_for(joiner_id, false);

        let start = tokio::time::Instant::now();
        while joiner.best_path_to_host().is_none() || host.connected_peers().is_empty() {
            assert!(start.elapsed() < Duration::from_secs(10), "ticket holders never linked");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(host.broadcast_lan_packet(b"W3GS GAMEINFO"), 1);
        assert!(joiner.send_datagram_to(host_id, b"opus".to_vec()));

        let start = tokio::time::Instant::now();
        let (mut lan, mut voice) = (Vec::new(), Vec::new());
        while lan.is_empty() || voice.is_empty() {
            assert!(start.elapsed() < Duration::from_secs(5), "datagrams never arrived");
            lan.extend(joiner.poll_lan_packets());
            voice.extend(host.poll_datagrams());
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(lan, vec![(host_id, b"W3GS GAMEINFO".to_vec())]);
        assert_eq!(voice, vec![(joiner_id, b"opus".to_vec())]);

        // Without a valid ticket nobody gets in.
        let intruder = PeerMesh::spawn_with(MeshConfig {
            rendezvous_addr: rv_addr,
            room_token: room,
            own_client_id: Uuid::new_v4(),
            is_host: false,
            ticket: Some(b"forged".to_vec()),
            relay_addr: None,
        });
        tokio::time::sleep(Duration::from_millis(800)).await;
        assert!(intruder.own_registration_addr().is_some());
        assert_eq!(host.peer_statuses().len(), 1, "the intruder must not be announced");

        host.abort();
        joiner.abort();
        intruder.abort();
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
