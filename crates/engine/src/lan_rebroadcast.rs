use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info, trace, warn};

/// Space-separated hex of `bytes`, for `trace!` dumps. Built only when asked
/// for: the callers check `tracing::enabled!` first, since a hex string per
/// packet is exactly what made the daemon's log huge at the default level.
pub(crate) fn hex_dump(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}
use warlay_transport::{TransportError, UdpTransport};

pub struct LanRebroadcastHandle {
    send_task: JoinHandle<()>,
    reply_task: JoinHandle<()>,
}

impl LanRebroadcastHandle {
    pub fn abort(self) {
        self.send_task.abort();
        self.reply_task.abort();
    }
}

/// Rebroadcasts, byte for byte, every real W3GS packet relayed to us from
/// another room member's actual Warcraft III (captured off the wire on
/// their end — see `network::lan_capture`) onto *this* machine's own local
/// network, so this machine's own WC3 sees the real thing on "Red de Área
/// Local" — not a guess at the protocol, the exact bytes WC3 itself
/// produced. Replaces the old synthesized-packet approach entirely.
///
/// Also listens for direct replies to what it just sent: `W3GS_GAMEINFO`
/// (the actual game-details packet, as opposed to the lifecycle broadcasts
/// `CREATEGAME`/`REFRESHGAME`/`DECREATEGAME`) appears to be sent by real
/// WC3 as a *unicast* reply straight back to whoever's `SEARCHGAME` it's
/// answering, rather than broadcast like its siblings — empirically, a real
/// hosting session relayed here shows the other three but never `GAMEINFO`
/// itself. Since we rebroadcast a room member's `SEARCHGAME` from this
/// socket, any such reply targets *this socket's own port*, which
/// `lan_capture`'s promiscuous listener (scoped to `game_port` only) would
/// never see — so this task, which owns that exact port, hands anything it
/// receives here to `on_reply` to relay onward instead.
pub async fn spawn(
    game_port: u16,
    mut packets: mpsc::UnboundedReceiver<Vec<u8>>,
    override_port: Option<u16>,
    on_reply: impl Fn(Vec<u8>) + Send + 'static,
) -> Result<LanRebroadcastHandle, TransportError> {
    let sender = Arc::new(UdpTransport::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0), true).await?);
    let loopback_sender =
        Arc::new(UdpTransport::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0), false).await?);
    let destination = UdpTransport::broadcast_addr(game_port);
    let loopback_destination = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), game_port);
    info!(%destination, local_addr = %sender.local_addr(), "LAN rebroadcaster ready, waiting for relayed W3GS packets from the hub");

    let send_task = tokio::spawn({
        let sender = sender.clone();
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
                        %destination,
                        "received a relayed W3GS packet from the hub — rebroadcasting it now onto our own local \
                         network so our WC3 can see it"
                    );
                    if tracing::enabled!(tracing::Level::TRACE) {
                        trace!(hex = %hex_dump(&packet), "relayed W3GS packet");
                    }
                }
                match sender.send_to(&packet, destination).await {
                    Ok(sent) => {
                        if received_count <= 3 {
                            info!(sent, %destination, "rebroadcast a relayed W3GS packet");
                        }
                    }
                    Err(error) => warn!(%error, "failed to rebroadcast a relayed LAN packet"),
                }
                match loopback_sender.send_to(&packet, loopback_destination).await {
                    Ok(sent) => {
                        if received_count <= 3 {
                            info!(sent, destination = %loopback_destination, "loopback rebroadcast a relayed W3GS packet");
                        }
                    }
                    Err(error) => warn!(%error, "failed to loopback rebroadcast a relayed LAN packet"),
                }
            }
            info!(received_count, "LAN rebroadcaster stopped");
        }
    });

    let reply_task = tokio::spawn(async move {
        let mut buffer = [0_u8; 8192];
        loop {
            match sender.recv_from(&mut buffer).await {
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
                            // broadcast/rebroadcast socket, so we just ignore it and keep listening.
                            continue;
                        }
                    }
                    warn!(%error, "rebroadcast socket's reply listener errored, stopping");
                    break;
                }
            }
        }
    });

    Ok(LanRebroadcastHandle { send_task, reply_task })
}
