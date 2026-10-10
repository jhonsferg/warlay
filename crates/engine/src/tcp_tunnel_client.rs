use super::p2p_mesh;
use super::p2p_tunnel;
use super::w3gs_sniff;
use rustls::pki_types::ServerName;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use uuid::Uuid;
use warlay_protocol::{ProtocolError, TunnelHello};

#[derive(Debug, Error)]
pub enum TunnelError {
    #[error("failed to connect to the tunnel hub at {addr}: {source}")]
    Connect {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    #[error("TLS handshake with the tunnel hub at {addr} failed: {source}")]
    Tls {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    #[error("'{0}' is not a valid TLS server name for the tunnel hub")]
    InvalidServerName(String),
    #[error("failed to bind local tunnel listener on {addr}: {source}")]
    Bind {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Where the hub's TCP game-session tunnel listens, and whether to reach
/// it over TLS. A bare `SocketAddr` converts into a plain endpoint, so
/// callers that never cared about TLS keep working unchanged.
#[derive(Clone)]
pub struct HubTunnelEndpoint {
    addr: SocketAddr,
    tls_server_name: Option<ServerName<'static>>,
}

impl HubTunnelEndpoint {
    pub fn plain(addr: SocketAddr) -> Self {
        Self {
            addr,
            tls_server_name: None,
        }
    }

    /// `server_name` is what the hub's certificate must be valid for --
    /// normally the hostname the user typed, not the resolved IP.
    pub fn tls(addr: SocketAddr, server_name: &str) -> Result<Self, TunnelError> {
        let tls_server_name = ServerName::try_from(server_name.to_string())
            .map_err(|_| TunnelError::InvalidServerName(server_name.to_string()))?;
        Ok(Self {
            addr,
            tls_server_name: Some(tls_server_name),
        })
    }

    async fn connect(&self) -> Result<HubStream, TunnelError> {
        let tcp = TcpStream::connect(self.addr)
            .await
            .map_err(|source| TunnelError::Connect {
                addr: self.addr,
                source,
            })?;
        let _ = tcp.set_nodelay(true);
        match &self.tls_server_name {
            None => Ok(Box::new(tcp)),
            Some(server_name) => {
                let connector = tokio_rustls::TlsConnector::from(super::tls::client_config());
                let tls = connector
                    .connect(server_name.clone(), tcp)
                    .await
                    .map_err(|source| TunnelError::Tls {
                        addr: self.addr,
                        source,
                    })?;
                Ok(Box::new(tls))
            }
        }
    }
}

impl From<SocketAddr> for HubTunnelEndpoint {
    fn from(addr: SocketAddr) -> Self {
        Self::plain(addr)
    }
}

impl fmt::Display for HubTunnelEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.tls_server_name {
            None => write!(f, "{}", self.addr),
            Some(server_name) => write!(f, "{} (tls, {})", self.addr, server_name.to_str()),
        }
    }
}

trait HubIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> HubIo for T {}

/// A connection to the hub's tunnel, plain or TLS -- everything past the
/// connect step only needs it to be a byte stream.
type HubStream = Box<dyn HubIo>;

pub struct TunnelHandle {
    tasks: Vec<JoinHandle<()>>,
}

impl TunnelHandle {
    pub fn abort(self) {
        for task in self.tasks {
            task.abort();
        }
    }
}

async fn send_hello<S: AsyncWrite + Unpin>(stream: &mut S, hello: &TunnelHello) -> Result<(), TunnelError> {
    let encoded = hello.encode()?;
    stream.write_all(&(encoded.len() as u32).to_le_bytes()).await?;
    stream.write_all(&encoded).await?;
    Ok(())
}

/// Runs on the room's host. Keeps exactly one control connection open to
/// the hub; every `0x01` byte read from it means "a joiner is waiting,"
/// which spawns a fresh backend bridge to the real WC3 host listener at
/// `127.0.0.1:local_game_port`.
///
/// `on_game_started` fires (best-effort — see `network::w3gs_sniff`'s docs)
/// the first time this host's own WC3 is seen emitting `COUNTDOWN_START` on
/// this bridge, i.e. the match has actually begun rather than still being
/// in the lobby. The caller uses it to auto-flip the room's status instead
/// of requiring the manual "Marcar en partida" click.
pub async fn start_hosting(
    hub_tcp_addr: impl Into<HubTunnelEndpoint>,
    room_token: Uuid,
    local_game_port: u16,
    on_game_started: impl Fn() + Send + Sync + 'static,
) -> Result<TunnelHandle, TunnelError> {
    let hub_tcp_addr = hub_tcp_addr.into();
    let mut control = hub_tcp_addr.connect().await?;
    send_hello(&mut control, &TunnelHello::HostControl { room_token }).await?;
    info!(%room_token, %hub_tcp_addr, "host tunnel control channel connected");

    let on_game_started = Arc::new(on_game_started);
    let task = tokio::spawn(async move {
        let mut signal = [0_u8; 1];
        loop {
            match control.read_exact(&mut signal).await {
                Ok(_) => {
                    tokio::spawn(open_host_backend(
                        hub_tcp_addr.clone(),
                        room_token,
                        local_game_port,
                        on_game_started.clone(),
                    ));
                }
                Err(_) => {
                    warn!(%room_token, "host tunnel control channel closed");
                    break;
                }
            }
        }
    });

    Ok(TunnelHandle { tasks: vec![task] })
}

/// Runs on the room's host. Always launches the TCP relay mechanism via `start_hosting`,
/// and if a PeerMesh is active (`mesh.is_some()`), additionally launches continuous bridging
/// for both direct and bridged P2P streams from all connected peers.
pub async fn start_hosting_with_p2p(
    hub_tcp_addr: impl Into<HubTunnelEndpoint>,
    room_token: Uuid,
    local_game_port: u16,
    mesh: Option<Arc<p2p_mesh::PeerMeshHandle>>,
    on_game_started: impl Fn() + Send + Sync + 'static,
) -> Result<TunnelHandle, TunnelError> {
    let hub_tcp_addr = hub_tcp_addr.into();
    let on_game_started = Arc::new(on_game_started);

    // The relay leg and the mesh leg are independent, best-effort paths --
    // a peer with no P2P/bridge connectivity yet still needs the relay to
    // work, and a healthy mesh shouldn't be sunk (or delayed!) by the relay
    // leg's own setup. Spawn the mesh-bridging task first, immediately --
    // it must not wait on `start_hosting`'s own TCP connect, which can take
    // a while to fail if the relay's control port isn't reachable, and
    // every second that delays the mesh task is a second the bridge/direct
    // P2P side sits unbridged even though it may already be fully connected.
    let mut tasks = Vec::new();
    let mesh_active = mesh.is_some();
    if let Some(mesh_handle) = mesh {
        let cb = on_game_started.clone();
        let host_mesh_task = tokio::spawn(async move {
            run_host_mesh_loop(mesh_handle, local_game_port, cb).await;
        });
        tasks.push(host_mesh_task);
    }

    let relay_result = start_hosting(hub_tcp_addr.clone(), room_token, local_game_port, {
        let cb = on_game_started.clone();
        move || cb()
    })
    .await;

    let mut relay_error = None;
    match relay_result {
        Ok(relay_handle) => tasks.extend(relay_handle.tasks),
        Err(err) => {
            warn!(%err, %hub_tcp_addr, %room_token, "failed to start TCP relay fallback for hosting");
            relay_error = Some(err);
        }
    }

    if let Some(err) = relay_error {
        if !mesh_active {
            return Err(err);
        }
    }

    Ok(TunnelHandle { tasks })
}

async fn connect_local_game(local_game_port: u16) -> Option<TcpStream> {
    let mut addrs_to_try = vec![std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))];
    for ipv4 in super::lan_capture::local_ipv4_addrs() {
        let ip = std::net::IpAddr::V4(ipv4);
        if !addrs_to_try.contains(&ip) {
            addrs_to_try.push(ip);
        }
    }

    for _attempt in 0..15 {
        for &ip in &addrs_to_try {
            let addr = SocketAddr::new(ip, local_game_port);
            if let Ok(stream) = TcpStream::connect(addr).await {
                let _ = stream.set_nodelay(true);
                return Some(stream);
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    None
}

async fn run_host_mesh_loop(
    mesh_handle: Arc<p2p_mesh::PeerMeshHandle>,
    local_game_port: u16,
    on_game_started: Arc<dyn Fn() + Send + Sync + 'static>,
) {
    let mut bridged_peers = std::collections::HashSet::new();
    let mut bridge_tasks = tokio::task::JoinSet::new();

    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        let connected = mesh_handle.connected_peers();
        for (peer_id, handle) in connected {
            if bridged_peers.insert(peer_id) {
                let cb = on_game_started.clone();
                bridge_tasks.spawn(async move {
                    run_host_peer_bridge(handle, local_game_port, cb).await;
                });
            }
        }
    }
}

async fn run_host_peer_bridge(
    handle: Arc<p2p_tunnel::P2pTunnelHandle>,
    local_game_port: u16,
    on_game_started: Arc<dyn Fn() + Send + Sync + 'static>,
) {
    // 1. Primary stream bridge -- reactive: a directly-connected mesh peer
    // that's purely acting as someone else's bridge (not playing itself,
    // or hasn't launched its own game yet) never writes anything on its
    // own primary stream. Eagerly opening a local WC3 backend connection
    // for every connected peer regardless would waste one of WC3's LAN
    // connection slots on a peer with nothing to send, so wait for real
    // bytes first, same pattern as the bridge-relay case in `p2p_mesh.rs`.
    let handle_primary = handle.clone();
    let on_game_started_primary = on_game_started.clone();
    let primary_task = tokio::spawn(async move {
        let mut first_buf = [0_u8; 8192];
        let n = {
            let mut recv_guard = handle_primary.recv.lock().await;
            match recv_guard.read(&mut first_buf).await {
                Ok(Some(n)) if n > 0 => n,
                _ => return,
            }
        };

        let Some(local_game) = connect_local_game(local_game_port).await else {
            warn!(
                local_game_port,
                "host primary P2P bridge failed to connect to local game"
            );
            return;
        };

        let (mut game_read, mut game_write) = local_game.into_split();
        if game_write.write_all(&first_buf[..n]).await.is_err() {
            return;
        }

        let handle_primary_1 = handle_primary.clone();
        let p2p_to_game = tokio::spawn(async move {
            let mut buf = [0_u8; 8192];
            loop {
                let n = match handle_primary_1.recv.lock().await.read(&mut buf).await {
                    Ok(Some(n)) if n > 0 => n,
                    Ok(Some(_)) => continue,
                    _ => break,
                };
                if game_write.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        });

        let handle_primary_2 = handle_primary.clone();
        let game_to_p2p = tokio::spawn(async move {
            let mut buf = [0_u8; 8192];
            let mut announced = false;
            loop {
                let n = match game_read.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                if !announced && w3gs_sniff::contains_countdown_start(&buf[..n]) {
                    announced = true;
                    info!("detected host COUNTDOWN_START via primary P2P stream");
                    on_game_started_primary();
                }
                if handle_primary_2.send.lock().await.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        });

        let _ = tokio::join!(p2p_to_game, game_to_p2p);
    });

    // 2. Accept-bi loop for bridged streams
    let connection = handle.connection.clone();
    let on_game_started_bi = on_game_started.clone();
    let accept_task = tokio::spawn(async move {
        loop {
            match connection.accept_bi().await {
                Ok((mut send_stream, mut recv_stream)) => {
                    let mut handshake = [0_u8; 1];
                    if recv_stream.read_exact(&mut handshake).await.is_err() {
                        continue;
                    }
                    if handshake[0] != 0x01 {
                        continue;
                    }

                    let on_game_started_stream = on_game_started_bi.clone();
                    tokio::spawn(async move {
                        let Some(local_game) = connect_local_game(local_game_port).await else {
                            warn!(
                                local_game_port,
                                "host bridged P2P stream failed to connect to local game"
                            );
                            return;
                        };

                        let (mut game_read, mut game_write) = local_game.into_split();

                        let quic_to_game = tokio::spawn(async move {
                            let mut buf = [0_u8; 8192];
                            loop {
                                let n = match recv_stream.read(&mut buf).await {
                                    Ok(Some(n)) if n > 0 => n,
                                    Ok(Some(_)) => continue,
                                    _ => break,
                                };
                                if game_write.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                        });

                        let game_to_quic = tokio::spawn(async move {
                            let mut buf = [0_u8; 8192];
                            let mut announced = false;
                            loop {
                                let n = match game_read.read(&mut buf).await {
                                    Ok(0) => break,
                                    Ok(n) => n,
                                    Err(_) => break,
                                };
                                if !announced && w3gs_sniff::contains_countdown_start(&buf[..n]) {
                                    announced = true;
                                    info!("detected host COUNTDOWN_START via bridged P2P stream");
                                    on_game_started_stream();
                                }
                                if send_stream.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                        });

                        let _ = tokio::join!(quic_to_game, game_to_quic);
                    });
                }
                Err(_) => break,
            }
        }
    });

    let _ = tokio::join!(primary_task, accept_task);
}

async fn open_host_backend(
    hub_tcp_addr: HubTunnelEndpoint,
    room_token: Uuid,
    local_game_port: u16,
    on_game_started: Arc<impl Fn() + Send + Sync + 'static>,
) {
    let mut backend = match hub_tcp_addr.connect().await {
        Ok(stream) => stream,
        Err(error) => {
            warn!(%error, %hub_tcp_addr, "failed to open tunnel backend connection");
            return;
        }
    };
    if let Err(error) = send_hello(&mut backend, &TunnelHello::HostBackend { room_token }).await {
        warn!(%error, "failed to send HostBackend hello");
        return;
    }

    let mut local_game = None;
    let mut addrs_to_try = vec![std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))];
    for ipv4 in super::lan_capture::local_ipv4_addrs() {
        let ip = std::net::IpAddr::V4(ipv4);
        if !addrs_to_try.contains(&ip) {
            addrs_to_try.push(ip);
        }
    }

    for ip in addrs_to_try {
        let addr = SocketAddr::new(ip, local_game_port);
        if let Ok(stream) = TcpStream::connect(addr).await {
            let _ = stream.set_nodelay(true);
            debug!(%addr, "successfully connected to local WC3 host listener");
            local_game = Some(stream);
            break;
        }
    }

    let local_game = match local_game {
        Some(stream) => stream,
        None => {
            warn!(
                local_game_port,
                "failed to connect to the local WC3 host listener on any local IP"
            );
            return;
        }
    };

    // A manual two-task pump instead of `tokio::io::copy_bidirectional` —
    // proxies bytes exactly the same way, but lets the `local_game ->
    // backend` direction (what this host's own WC3 is sending out) get
    // peeked by `w3gs_sniff` on the way through. Never mutates or delays a
    // byte; a sniffing bug can at worst fail to *detect* something, it
    // cannot corrupt or drop what's being proxied.
    let (mut backend_read, mut backend_write) = tokio::io::split(backend);
    let (mut game_read, mut game_write) = local_game.into_split();

    let hub_to_game = tokio::spawn(async move {
        let mut buf = [0_u8; 8192];
        let mut total = 0_u64;
        loop {
            let n = match backend_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            if game_write.write_all(&buf[..n]).await.is_err() {
                break;
            }
            total += n as u64;
        }
        total
    });

    let game_to_hub = tokio::spawn(async move {
        let mut buf = [0_u8; 8192];
        let mut total = 0_u64;
        let mut announced = false;
        loop {
            let n = match game_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            if !announced && w3gs_sniff::contains_countdown_start(&buf[..n]) {
                announced = true;
                info!(%room_token, "detected this host's own COUNTDOWN_START, marking the room in progress");
                on_game_started();
            }
            if backend_write.write_all(&buf[..n]).await.is_err() {
                break;
            }
            total += n as u64;
        }
        total
    });

    match tokio::try_join!(hub_to_game, game_to_hub) {
        Ok((from_hub, from_game)) => {
            debug!(%room_token, from_hub, from_game, "tunnel backend closed");
        }
        Err(error) => debug!(%error, %room_token, "tunnel backend task panicked"),
    }
}

/// Runs on a joiner. Listens locally on `local_listen_addr` — the port the
/// synthesized LAN announcement told WC3 to connect to — and bridges every
/// connection WC3 makes there to a fresh stream on the hub, which the hub
/// pairs with the host's next backend connection.
pub async fn start_joining(
    hub_tcp_addr: impl Into<HubTunnelEndpoint>,
    room_token: Uuid,
    local_listen_addr: SocketAddr,
) -> Result<(TunnelHandle, u16), TunnelError> {
    let hub_tcp_addr = hub_tcp_addr.into();
    let listener = TcpListener::bind(local_listen_addr)
        .await
        .map_err(|source| TunnelError::Bind {
            addr: local_listen_addr,
            source,
        })?;

    let actual_port = listener
        .local_addr()
        .map(|a| a.port())
        .unwrap_or(local_listen_addr.port());
    info!(%local_listen_addr, actual_port, %room_token, "joiner tunnel listening for the local game client");

    let task = tokio::spawn(async move {
        loop {
            let (local_game, peer_addr) = match listener.accept().await {
                Ok(result) => {
                    let _ = result.0.set_nodelay(true);
                    result
                }
                Err(error) => {
                    warn!(%error, "failed to accept local game connection");
                    continue;
                }
            };
            debug!(%peer_addr, %room_token, "local game client connected, bridging to hub");
            tokio::spawn(bridge_joiner_stream(hub_tcp_addr.clone(), room_token, local_game));
        }
    });

    Ok((TunnelHandle { tasks: vec![task] }, actual_port))
}

/// Runs on a joiner. Listens locally on `local_listen_addr`. If a PeerMesh is active
/// (`mesh.is_some()`), queries `best_path_to_host()` to bridge local game client traffic over P2P.
/// If P2P path is unavailable or times out, falls back to server TCP relay via `bridge_joiner_stream`.
pub async fn start_joining_with_p2p(
    hub_tcp_addr: impl Into<HubTunnelEndpoint>,
    room_token: Uuid,
    local_listen_addr: SocketAddr,
    mesh: Option<Arc<p2p_mesh::PeerMeshHandle>>,
) -> Result<(TunnelHandle, u16), TunnelError> {
    let hub_tcp_addr = hub_tcp_addr.into();
    let listener = TcpListener::bind(local_listen_addr)
        .await
        .map_err(|source| TunnelError::Bind {
            addr: local_listen_addr,
            source,
        })?;

    let actual_port = listener
        .local_addr()
        .map(|a| a.port())
        .unwrap_or(local_listen_addr.port());
    info!(%local_listen_addr, actual_port, %room_token, "joiner tunnel listening for local game client with P2P support");

    let task = tokio::spawn(async move {
        loop {
            let (local_game, peer_addr) = match listener.accept().await {
                Ok(result) => {
                    let _ = result.0.set_nodelay(true);
                    result
                }
                Err(error) => {
                    warn!(%error, "failed to accept local game connection in P2P mode");
                    continue;
                }
            };
            debug!(%peer_addr, %room_token, "local game client connected, attempting P2P mesh bridge");
            let mesh_clone = mesh.clone();
            let hub_tcp_addr = hub_tcp_addr.clone();
            tokio::spawn(async move {
                bridge_joiner_connection(hub_tcp_addr, room_token, local_game, mesh_clone).await;
            });
        }
    });

    Ok((TunnelHandle { tasks: vec![task] }, actual_port))
}

async fn bridge_joiner_connection(
    hub_tcp_addr: HubTunnelEndpoint,
    room_token: Uuid,
    local_game: TcpStream,
    mesh: Option<Arc<p2p_mesh::PeerMeshHandle>>,
) {
    if let Some(mesh_handle) = mesh {
        let start = tokio::time::Instant::now();
        let mut path_handle = None;

        while start.elapsed() < Duration::from_secs(4) {
            if let Some(path) = mesh_handle.best_path_to_host() {
                let handle = match path {
                    p2p_mesh::PathToHost::Direct(h) => h,
                    p2p_mesh::PathToHost::ViaPeer(_, h) => h,
                };
                path_handle = Some(handle);
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if let Some(mut handle) = path_handle {
            info!(%room_token, "bridging joiner stream over P2P mesh");
            let mut local_game = local_game;
            loop {
                if pump_joiner_p2p(&mut local_game, &handle).await.is_ok() {
                    return;
                }
                warn!(%room_token, "P2P joiner stream interrupted, searching for alternative path");
                if handle.connection.close_reason().is_some() {
                    if let Some(new_path) = mesh_handle.best_path_to_host() {
                        let new_handle = match new_path {
                            p2p_mesh::PathToHost::Direct(h) => h,
                            p2p_mesh::PathToHost::ViaPeer(_, h) => h,
                        };
                        handle = new_handle;
                        info!(%room_token, "switched joiner stream to alternative P2P path");
                        continue;
                    }
                }
                break;
            }
            return;
        } else {
            warn!(%room_token, "no P2P path to host found within 4s timeout, falling back to server TCP relay");
        }
    }

    bridge_joiner_stream(hub_tcp_addr, room_token, local_game).await;
}

async fn pump_joiner_p2p(local_game: &mut TcpStream, handle: &Arc<p2p_tunnel::P2pTunnelHandle>) -> Result<(), ()> {
    let (mut game_read, mut game_write) = local_game.split();

    let game_to_p2p = async {
        let mut buf = [0_u8; 8192];
        loop {
            let n = match game_read.read(&mut buf).await {
                Ok(0) => break Ok(()),
                Ok(n) => n,
                Err(_) => break Err(()),
            };
            if handle.send.lock().await.write_all(&buf[..n]).await.is_err() {
                break Err(());
            }
        }
    };

    let p2p_to_game = async {
        let mut buf = [0_u8; 8192];
        loop {
            let n = match handle.recv.lock().await.read(&mut buf).await {
                Ok(Some(n)) if n > 0 => n,
                Ok(Some(_)) => continue,
                _ => break Err(()),
            };
            if game_write.write_all(&buf[..n]).await.is_err() {
                break Err(());
            }
        }
    };

    tokio::select! {
        res1 = game_to_p2p => res1,
        res2 = p2p_to_game => res2,
    }
}

async fn bridge_joiner_stream(hub_tcp_addr: HubTunnelEndpoint, room_token: Uuid, mut local_game: TcpStream) {
    let mut hub_stream = match hub_tcp_addr.connect().await {
        Ok(stream) => stream,
        Err(error) => {
            warn!(%error, %hub_tcp_addr, "failed to open joiner tunnel stream");
            return;
        }
    };
    if let Err(error) = send_hello(&mut hub_stream, &TunnelHello::JoinerStream { room_token }).await {
        warn!(%error, "failed to send JoinerStream hello");
        return;
    }

    match tokio::io::copy_bidirectional(&mut local_game, &mut hub_stream).await {
        Ok((from_game, from_hub)) => {
            debug!(%room_token, from_game, from_hub, "joiner tunnel stream closed");
        }
        Err(error) => debug!(%error, %room_token, "joiner tunnel stream error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{timeout, Duration};

    /// Exercises `start_hosting`/`start_joining` against the real deployed
    /// `warlay-hub` VPS, standing in for WC3 on both ends with plain TCP
    /// sockets. Not run by default (needs live network access to a specific
    /// host); run explicitly with `cargo test -- --ignored` when validating
    /// a deployment.
    #[tokio::test]
    #[ignore = "requires the deployed warlay-hub VPS at 161.132.37.213:7778"]
    async fn full_round_trip_against_live_vps() {
        let hub_tcp_addr: SocketAddr = "161.132.37.213:7778".parse().unwrap();
        let room_token = Uuid::new_v4();

        let host_stub = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host_stub_port = host_stub.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut socket, _) = host_stub.accept().await.unwrap();
            let mut buf = [0_u8; 5];
            socket.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping!");
            socket.write_all(b"pong!").await.unwrap();
        });

        let _host_handle = start_hosting(hub_tcp_addr, room_token, host_stub_port, || {})
            .await
            .expect("start_hosting should connect");
        tokio::time::sleep(Duration::from_millis(300)).await;

        let joiner_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let joiner_listen_addr = joiner_listener.local_addr().unwrap();
        drop(joiner_listener); // free the port for start_joining to rebind

        let _joiner_handle = start_joining(hub_tcp_addr, room_token, joiner_listen_addr)
            .await
            .expect("start_joining should bind");
        tokio::time::sleep(Duration::from_millis(200)).await;

        let mut joiner_client = TcpStream::connect(joiner_listen_addr).await.unwrap();
        joiner_client.write_all(b"ping!").await.unwrap();

        let mut response = [0_u8; 5];
        timeout(Duration::from_secs(5), joiner_client.read_exact(&mut response))
            .await
            .expect("timed out waiting for end-to-end response")
            .expect("read response");
        assert_eq!(&response, b"pong!");
    }

    /// Same round trip, with both legs TLS-wrapped through the hub's
    /// reverse proxy (`:7443` terminates TLS in front of the plain tunnel).
    #[tokio::test]
    #[ignore = "requires the deployed warlay-hub at warlay.codesoftlabs.com:7443"]
    async fn full_round_trip_over_tls_against_live_hub() {
        let hub_host = "warlay.codesoftlabs.com";
        let addr = tokio::net::lookup_host((hub_host, 7443)).await.unwrap().next().unwrap();
        let hub_tcp_addr = HubTunnelEndpoint::tls(addr, hub_host).unwrap();
        let room_token = Uuid::new_v4();

        let host_stub = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host_stub_port = host_stub.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut socket, _) = host_stub.accept().await.unwrap();
            let mut buf = [0_u8; 5];
            socket.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping!");
            socket.write_all(b"pong!").await.unwrap();
        });

        let _host_handle = start_hosting(hub_tcp_addr.clone(), room_token, host_stub_port, || {})
            .await
            .expect("start_hosting should connect over TLS");
        tokio::time::sleep(Duration::from_millis(300)).await;

        let joiner_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let joiner_listen_addr = joiner_listener.local_addr().unwrap();
        drop(joiner_listener);

        let _joiner_handle = start_joining(hub_tcp_addr, room_token, joiner_listen_addr)
            .await
            .expect("start_joining should bind");
        tokio::time::sleep(Duration::from_millis(200)).await;

        let mut joiner_client = TcpStream::connect(joiner_listen_addr).await.unwrap();
        joiner_client.write_all(b"ping!").await.unwrap();

        let mut response = [0_u8; 5];
        timeout(Duration::from_secs(5), joiner_client.read_exact(&mut response))
            .await
            .expect("timed out waiting for end-to-end response over TLS")
            .expect("read response");
        assert_eq!(&response, b"pong!");
    }

    #[tokio::test]
    async fn p2p_hosting_and_joining_end_to_end_local() {
        use tokio::net::UdpSocket;
        use warlay_core::{UdpRendezvous, UdpRendezvousHandle};

        let rv_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let rv_addr = rv_socket.local_addr().unwrap();
        let rendezvous = Arc::new(UdpRendezvous::new(rv_socket));
        let _rv_handle: UdpRendezvousHandle = rendezvous.spawn();

        let hub_tcp_addr = SocketAddr::new(rv_addr.ip(), rv_addr.port() - 1);
        let room_token = Uuid::new_v4();

        let host_id = Uuid::new_v4();
        let joiner_id = Uuid::new_v4();

        let host_mesh = Arc::new(p2p_mesh::PeerMesh::spawn(rv_addr, room_token, host_id, true));
        let joiner_mesh = Arc::new(p2p_mesh::PeerMesh::spawn(rv_addr, room_token, joiner_id, false));

        let start = tokio::time::Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if joiner_mesh.best_path_to_host().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(
            joiner_mesh.best_path_to_host().is_some(),
            "joiner should connect directly to host"
        );

        // Host local game stub
        let host_stub = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host_stub_port = host_stub.local_addr().unwrap().port();

        let (host_send_tx, mut host_recv_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(1);

        tokio::spawn(async move {
            let (mut socket, _) = host_stub.accept().await.unwrap();
            let mut buf = [0_u8; 16];
            let n = socket.read(&mut buf).await.unwrap();
            host_send_tx.send(buf[..n].to_vec()).await.unwrap();
            socket.write_all(b"p2p-reply-payload").await.unwrap();
        });

        let host_mesh_clone = host_mesh.clone();
        let host_task = tokio::spawn(start_hosting_with_p2p(
            hub_tcp_addr,
            room_token,
            host_stub_port,
            Some(host_mesh_clone),
            || {},
        ));

        let joiner_mesh_clone = joiner_mesh.clone();
        let joiner_task = tokio::spawn(start_joining_with_p2p(
            hub_tcp_addr,
            room_token,
            SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0),
            Some(joiner_mesh_clone),
        ));

        let (host_res, joiner_res) = tokio::join!(host_task, joiner_task);

        let _host_handle = host_res.unwrap().expect("host P2P start should succeed");
        let (_joiner_handle, actual_port) = joiner_res.unwrap().expect("joiner P2P start should succeed");

        let mut joiner_client = TcpStream::connect(SocketAddr::new("127.0.0.1".parse().unwrap(), actual_port))
            .await
            .unwrap();
        joiner_client.write_all(b"p2p-test-payload").await.unwrap();

        let host_received = timeout(Duration::from_secs(5), host_recv_rx.recv())
            .await
            .expect("host should receive payload in time")
            .unwrap();
        assert_eq!(&host_received, b"p2p-test-payload");

        let mut reply_buf = [0_u8; 17];
        timeout(Duration::from_secs(5), joiner_client.read_exact(&mut reply_buf))
            .await
            .expect("joiner should receive reply in time")
            .unwrap();
        assert_eq!(&reply_buf, b"p2p-reply-payload");

        host_mesh.abort();
        joiner_mesh.abort();
    }

    #[tokio::test]
    async fn p2p_1hop_bridge_end_to_end_local() {
        use tokio::net::UdpSocket;
        use warlay_core::{UdpRendezvous, UdpRendezvousHandle};

        let rv1_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let rv1_addr = rv1_socket.local_addr().unwrap();
        let rv1 = Arc::new(UdpRendezvous::new(rv1_socket));
        let _rv1_handle: UdpRendezvousHandle = rv1.spawn();

        let mock_rv_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mock_rv_addr = mock_rv_socket.local_addr().unwrap();

        let hub_tcp_addr = SocketAddr::new(rv1_addr.ip(), rv1_addr.port() - 1);
        let room_token = Uuid::new_v4();

        let host_id = Uuid::new_v4();
        let bridge_id = Uuid::new_v4();
        let joiner_id = Uuid::new_v4();

        let host_mesh = Arc::new(p2p_mesh::PeerMesh::spawn(rv1_addr, room_token, host_id, true));
        let bridge_mesh = Arc::new(p2p_mesh::PeerMesh::spawn(rv1_addr, room_token, bridge_id, false));

        // The mock rendezvous below needs the bridge's *actual* P2P
        // registration address (not the real rendezvous server's own
        // address -- sending punches/offers there would just hit the
        // rendezvous socket, which silently drops anything it can't
        // decode as a RendezvousMessage, so the joiner would never
        // reach the bridge at all).
        let bridge_reg_addr = {
            let start = tokio::time::Instant::now();
            loop {
                if let Some(addr) = bridge_mesh.own_registration_addr() {
                    let ip = if addr.ip().is_unspecified() {
                        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
                    } else {
                        addr.ip()
                    };
                    break SocketAddr::new(ip, addr.port());
                }
                assert!(start.elapsed() < Duration::from_secs(5), "bridge_mesh never registered");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };

        // Mock rendezvous for joiner: returns unreachable IP for host, the bridge's real address for the bridge.
        let _mock_rv_task = tokio::spawn(async move {
            let mut buf = [0_u8; 2048];
            loop {
                if let Ok((len, src)) = mock_rv_socket.recv_from(&mut buf).await {
                    if let Ok(msg) = warlay_protocol::RendezvousMessage::decode(&buf[..len]) {
                        match msg {
                            warlay_protocol::RendezvousMessage::Register { .. }
                            | warlay_protocol::RendezvousMessage::Heartbeat { .. } => {
                                let peer_host = warlay_protocol::PeerInfo {
                                    client_id: host_id,
                                    addr: "192.0.2.1:12345".parse().unwrap(),
                                    is_host: true,
                                };
                                let peer_bridge = warlay_protocol::PeerInfo {
                                    client_id: bridge_id,
                                    addr: bridge_reg_addr,
                                    is_host: false,
                                };
                                let reply = warlay_protocol::RendezvousMessage::PeerList {
                                    peers: vec![peer_host, peer_bridge],
                                };
                                if let Ok(bytes) = reply.encode() {
                                    let _ = mock_rv_socket.send_to(&bytes, src).await;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        let joiner_mesh = Arc::new(p2p_mesh::PeerMesh::spawn(mock_rv_addr, room_token, joiner_id, false));

        // Wait for joiner_mesh to establish ViaPeer path to host
        let start = tokio::time::Instant::now();
        while start.elapsed() < Duration::from_secs(6) {
            if let Some(p2p_mesh::PathToHost::ViaPeer(pid, _)) = joiner_mesh.best_path_to_host() {
                if pid == bridge_id {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let path = joiner_mesh
            .best_path_to_host()
            .expect("joiner should obtain ViaPeer path");
        match path {
            p2p_mesh::PathToHost::ViaPeer(pid, _) => assert_eq!(pid, bridge_id),
            other => panic!("expected ViaPeer path, got {:?}", other),
        }

        // Host local game stub
        let host_stub = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host_stub_port = host_stub.local_addr().unwrap().port();

        let (host_send_tx, mut host_recv_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(1);

        tokio::spawn(async move {
            let (mut socket, _) = host_stub.accept().await.unwrap();
            let mut buf = [0_u8; 16];
            let n = socket.read(&mut buf).await.unwrap();
            host_send_tx.send(buf[..n].to_vec()).await.unwrap();
            socket.write_all(b"1hop-pong-reply").await.unwrap();
        });

        let host_mesh_clone = host_mesh.clone();
        let host_task = tokio::spawn(start_hosting_with_p2p(
            hub_tcp_addr,
            room_token,
            host_stub_port,
            Some(host_mesh_clone),
            || {},
        ));

        let joiner_mesh_clone = joiner_mesh.clone();
        let joiner_task = tokio::spawn(start_joining_with_p2p(
            hub_tcp_addr,
            room_token,
            SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0),
            Some(joiner_mesh_clone),
        ));

        let (host_res, joiner_res) = tokio::join!(host_task, joiner_task);

        let _host_handle = host_res.unwrap().expect("host P2P start should succeed");
        let (_joiner_handle, actual_port) = joiner_res.unwrap().expect("joiner P2P start should succeed");

        let mut joiner_client = TcpStream::connect(SocketAddr::new("127.0.0.1".parse().unwrap(), actual_port))
            .await
            .unwrap();
        joiner_client.write_all(b"1hop-ping-data!").await.unwrap();

        let host_received = timeout(Duration::from_secs(5), host_recv_rx.recv())
            .await
            .expect("host should receive bridged payload in time")
            .unwrap();
        assert_eq!(&host_received, b"1hop-ping-data!");

        let mut reply_buf = [0_u8; 15];
        timeout(Duration::from_secs(5), joiner_client.read_exact(&mut reply_buf))
            .await
            .expect("joiner should receive 1-hop reply in time")
            .unwrap();
        assert_eq!(&reply_buf, b"1hop-pong-reply");

        host_mesh.abort();
        bridge_mesh.abort();
        joiner_mesh.abort();
    }
}
