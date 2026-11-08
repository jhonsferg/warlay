use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info, trace, warn};

/// Space-separated hex of `bytes`, for `trace!` dumps. Built only when asked
/// for: the callers check `tracing::enabled!` first, since a hex string per
/// packet is exactly what made the client's log huge at the default level.
pub(crate) fn hex_dump(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}
use warlay_transport::{TransportError, UdpTransport};

pub struct LanRebroadcastHandle {
    send_task: JoinHandle<()>,
    reply_tasks: Vec<JoinHandle<()>>,
}

impl LanRebroadcastHandle {
    pub fn abort(self) {
        self.send_task.abort();
        for task in self.reply_tasks {
            task.abort();
        }
    }
}

/// Environment switch that ALSO replays onto the physical LAN broadcast
/// address (legacy behaviour). Off by default: see `spawn`.
const BROADCAST_ENV: &str = "WARLAY_LAN_BROADCAST";

fn broadcast_enabled() -> bool {
    std::env::var(BROADCAST_ENV).is_ok_and(|v| matches!(v.trim(), "1" | "true" | "yes"))
}

/// Replays, byte for byte, every real W3GS packet relayed to us from another
/// room member's actual Warcraft III (captured off the wire on their end --
/// see `network::lan_capture`) so this machine's own WC3 sees the real thing
/// on "Red de Área Local" -- not a guess at the protocol, the exact bytes WC3
/// itself produced.
///
/// **One delivery path, on purpose: loopback (`127.0.0.1:game_port`).** WC3
/// listens on `0.0.0.0:game_port`, so the datagram reaches it, and it lists a
/// game once per distinct *sender address* it hears. Replaying the same packet
/// both to the LAN broadcast address and to loopback made every game show up
/// twice (once per sender: the LAN IP and `127.0.0.1`). Loopback alone also
/// keeps remote games from leaking onto the user's real LAN and from being
/// captured back by our own promiscuous capture. Set `WARLAY_LAN_BROADCAST=1`
/// to add the broadcast copy again if a setup needs it.
///
/// Also listens for direct replies to what it just sent: `W3GS_GAMEINFO`
/// (the actual game-details packet, as opposed to the lifecycle broadcasts
/// `CREATEGAME`/`REFRESHGAME`/`DECREATEGAME`) is sent by real WC3 as a
/// *unicast* reply straight back to whoever's `SEARCHGAME` it's answering.
/// Since we replay a room member's `SEARCHGAME` from our own socket, such a
/// reply targets *that socket's own port* -- so each sending socket also
/// listens, and hands anything it receives to `on_reply` to relay onward.
pub async fn spawn(
    game_port: u16,
    packets: mpsc::UnboundedReceiver<Vec<u8>>,
    override_port: Option<u16>,
    on_reply: impl Fn(Vec<u8>) + Send + Sync + 'static,
) -> Result<LanRebroadcastHandle, TransportError> {
    spawn_with(game_port, packets, override_port, on_reply, broadcast_enabled()).await
}

pub(crate) async fn spawn_with(
    game_port: u16,
    mut packets: mpsc::UnboundedReceiver<Vec<u8>>,
    override_port: Option<u16>,
    on_reply: impl Fn(Vec<u8>) + Send + Sync + 'static,
    also_broadcast: bool,
) -> Result<LanRebroadcastHandle, TransportError> {
    let loopback_sender =
        Arc::new(UdpTransport::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0), false).await?);
    let loopback_destination = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), game_port);
    let broadcast_sender = if also_broadcast {
        Some(Arc::new(
            UdpTransport::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0), true).await?,
        ))
    } else {
        None
    };
    let destination = UdpTransport::broadcast_addr(game_port);
    info!(
        %loopback_destination,
        local_addr = %loopback_sender.local_addr(),
        also_broadcast,
        "LAN rebroadcaster ready, waiting for relayed W3GS packets from the hub"
    );

    let send_task = tokio::spawn({
        let loopback_sender = loopback_sender.clone();
        let broadcast_sender = broadcast_sender.clone();
        async move {
            let mut received_count: u64 = 0;
            while let Some(mut packet) = packets.recv().await {
                received_count += 1;

                // If this is a W3GS_GAMEINFO (0xF7 0x30), overwrite its port
                // so WC3 connects to our TCP tunnel instead of the original port
                if let Some(port) = override_port {
                    if packet.len() >= 4 && packet[0] == 0xF7 && packet[1] == 0x30 {
                        let len = packet.len();
                        let port_bytes = port.to_le_bytes();
                        packet[len - 2] = port_bytes[0];
                        packet[len - 1] = port_bytes[1];
                        debug!(port, "injected override TCP port into GAMEINFO broadcast");
                    }
                }

                // Full hex specifically for anything bigger than the small
                // lifecycle broadcasts (SEARCHGAME/REFRESHGAME/etc. are all
                // <20 bytes) — that's GAMEINFO, the one carrying the actual
                // game name/port WC3 uses to decide where to connect on join.
                if received_count == 1 || packet.len() > 20 {
                    debug!(
                        bytes = packet.len(),
                        destination = %loopback_destination,
                        "received a relayed W3GS packet from the hub — replaying it now onto our own machine so our \
                         WC3 can see it"
                    );
                    if tracing::enabled!(tracing::Level::TRACE) {
                        trace!(hex = %hex_dump(&packet), "relayed W3GS packet");
                    }
                }
                match loopback_sender.send_to(&packet, loopback_destination).await {
                    Ok(sent) => {
                        if received_count <= 3 {
                            info!(sent, destination = %loopback_destination, "replayed a relayed W3GS packet");
                        }
                    }
                    Err(error) => warn!(%error, "failed to replay a relayed LAN packet"),
                }
                if let Some(broadcast_sender) = &broadcast_sender {
                    if let Err(error) = broadcast_sender.send_to(&packet, destination).await {
                        warn!(%error, "failed to broadcast a relayed LAN packet");
                    }
                }
            }
            info!(received_count, "LAN rebroadcaster stopped");
        }
    });

    let on_reply = Arc::new(on_reply);
    let mut reply_tasks = vec![spawn_reply_listener(loopback_sender, on_reply.clone())];
    if let Some(broadcast_sender) = broadcast_sender {
        reply_tasks.push(spawn_reply_listener(broadcast_sender, on_reply));
    }

    Ok(LanRebroadcastHandle { send_task, reply_tasks })
}

/// Hands anything that arrives on one of our sending sockets (a real WC3's
/// unicast `GAMEINFO` answering a replayed `SEARCHGAME`) to `on_reply`.
fn spawn_reply_listener(
    socket: Arc<UdpTransport>,
    on_reply: Arc<impl Fn(Vec<u8>) + Send + Sync + 'static>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut buffer = [0_u8; 8192];
        loop {
            match socket.recv_from(&mut buffer).await {
                Ok((len, from)) => {
                    let payload = buffer[..len].to_vec();
                    debug!(
                        bytes = len,
                        %from,
                        "got a direct reply on our rebroadcast socket (likely a real W3GS_GAMEINFO answering a \
                         relayed SEARCHGAME) — relaying it onward"
                    );
                    if tracing::enabled!(tracing::Level::TRACE) {
                        trace!(hex = %hex_dump(&payload), "direct reply payload");
                    }
                    on_reply(payload);
                }
                Err(error) => {
                    if let TransportError::Io(ref io_err) = error {
                        if io_err.kind() == std::io::ErrorKind::ConnectionReset {
                            // On Windows, a UDP send_to that hits a closed port can cause the next
                            // recv_from to return WSAECONNRESET (10054). This is harmless for a
                            // rebroadcast socket, so we just ignore it and keep listening.
                            continue;
                        }
                    }
                    warn!(%error, "rebroadcast socket's reply listener errored, stopping");
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::net::UdpSocket;

    const SEARCH: [u8; 8] = [0xF7, 0x2F, 0x08, 0x00, 1, 2, 3, 4];

    /// A stand-in for WC3: bound on loopback like the real one is reachable
    /// on, counting every datagram it is shown and who it came from.
    async fn fake_wc3() -> (Arc<UdpSocket>, u16) {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let port = socket.local_addr().unwrap().port();
        (Arc::new(socket), port)
    }

    async fn collect(socket: &UdpSocket, window: Duration) -> Vec<(Vec<u8>, SocketAddr)> {
        let mut seen = Vec::new();
        let mut buf = [0u8; 2048];
        let end = tokio::time::Instant::now() + window;
        while let Ok(Ok((n, from))) = tokio::time::timeout_at(end, socket.recv_from(&mut buf)).await {
            seen.push((buf[..n].to_vec(), from));
        }
        seen
    }

    #[tokio::test]
    async fn a_relayed_packet_reaches_our_wc3_exactly_once() {
        // Regression: it used to be sent twice (LAN broadcast + loopback),
        // from two different sender addresses, so WC3 listed every game twice.
        let (wc3, port) = fake_wc3().await;
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = spawn_with(port, rx, None, |_| {}, false).await.unwrap();

        tx.send(SEARCH.to_vec()).unwrap();
        let seen = collect(&wc3, Duration::from_millis(400)).await;

        assert_eq!(seen.len(), 1, "one copy, one sender: {seen:?}");
        assert_eq!(seen[0].0, SEARCH);
        assert!(seen[0].1.ip().is_loopback());
        handle.abort();
    }

    #[tokio::test]
    async fn a_unicast_reply_from_wc3_is_relayed_onward() {
        // WC3 answers a SEARCHGAME with a unicast GAMEINFO to the sender: the
        // sending socket must be the one listening for it.
        let (wc3, port) = fake_wc3().await;
        let replies = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let sink = replies.clone();
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = spawn_with(port, rx, None, move |bytes| sink.lock().unwrap().push(bytes), false)
            .await
            .unwrap();

        tx.send(SEARCH.to_vec()).unwrap();
        let mut buf = [0u8; 64];
        let (_, from) = tokio::time::timeout(Duration::from_secs(2), wc3.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();
        wc3.send_to(&[0xF7, 0x30, 0x06, 0x00, 0xAA, 0xBB], from).await.unwrap();

        let start = tokio::time::Instant::now();
        while replies.lock().unwrap().is_empty() {
            assert!(start.elapsed() < Duration::from_secs(2), "reply never relayed");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(*replies.lock().unwrap(), vec![vec![0xF7, 0x30, 0x06, 0x00, 0xAA, 0xBB]]);
        handle.abort();
    }

    #[tokio::test]
    async fn packets_keep_their_order_and_bytes() {
        let (wc3, port) = fake_wc3().await;
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = spawn_with(port, rx, None, |_| {}, false).await.unwrap();

        let packets: Vec<Vec<u8>> = (0..5u8).map(|i| vec![0xF7, 0x32, 0x06, 0x00, i, 0xFF]).collect();
        for p in &packets {
            tx.send(p.clone()).unwrap();
        }
        let seen: Vec<Vec<u8>> = collect(&wc3, Duration::from_millis(400))
            .await
            .into_iter()
            .map(|(bytes, _)| bytes)
            .collect();
        assert_eq!(seen, packets);
        handle.abort();
    }

    #[tokio::test]
    async fn the_legacy_override_port_still_rewrites_a_gameinfo() {
        let (wc3, port) = fake_wc3().await;
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = spawn_with(port, rx, Some(4242), |_| {}, false).await.unwrap();

        tx.send(vec![0xF7, 0x30, 0x08, 0x00, 9, 9, 0x12, 0x34]).unwrap();
        let seen = collect(&wc3, Duration::from_millis(400)).await;
        assert_eq!(seen.len(), 1);
        assert_eq!(&seen[0].0[6..], &4242u16.to_le_bytes());
        handle.abort();
    }
}
