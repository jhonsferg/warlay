//! Linux counterpart of the Windows `SIO_RCVALL` capture: an `AF_PACKET`
//! socket sees every IP datagram this machine sends, on every interface,
//! including its own outgoing broadcasts (`PACKET_OUTGOING`) -- which is
//! exactly what Warcraft III's "Red de Área Local" announcements are. Same
//! API as the Windows module, so the daemon doesn't care which it runs on.
//!
//! Needs `CAP_NET_RAW` (root, or the capability granted to the binary),
//! the same way the Windows capture needs Administrator.

use socket2::{Domain, Protocol, Socket, Type};
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle as ThreadHandle;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info, warn};

use crate::lan_filter::{classify_packet, CaptureOutcome, CapturePorts};
use crate::subnet::local_ipv4_addrs;

const RECV_TIMEOUT: Duration = Duration::from_millis(500);
/// The largest possible IP datagram: nothing sent to the game port can be
/// bigger, and an `AF_PACKET` `SOCK_DGRAM` read hands us the IP packet.
const RECV_BUF_LEN: usize = 65535;
/// `ETH_P_ALL`: the kernel hands *outgoing* frames only to sockets tapping
/// every protocol (a socket bound to `ETH_P_IP` sees incoming traffic only).
const ETH_P_ALL: u16 = 0x0003;
/// `ETH_P_IP`: what we keep of it.
const ETH_P_IP: u16 = 0x0800;
/// `sll_pkttype` of a frame this host sent (as opposed to received).
const PACKET_OUTGOING: u8 = 4;

#[derive(Debug, Error)]
pub enum LanCaptureError {
    #[error("could not open a packet capture socket (does the daemon have CAP_NET_RAW?): {0}")]
    Socket(#[source] std::io::Error),
}

pub struct LanCaptureHandle {
    stop: Arc<AtomicBool>,
    /// Kept only to own the thread; `abort` doesn't join it.
    _thread: ThreadHandle<()>,
}

impl LanCaptureHandle {
    /// Signals the capture thread to stop and returns right away; it exits
    /// within `RECV_TIMEOUT`, when its blocking read wakes up.
    pub fn abort(self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// `struct sockaddr_ll` (linux/if_packet.h), to read the packet type of
/// what was received.
#[repr(C)]
struct SockaddrLl {
    family: u16,
    protocol: u16,
    ifindex: i32,
    hatype: u16,
    pkttype: u8,
    halen: u8,
    addr: [u8; 8],
}

/// Captures this machine's own outgoing UDP packets to one of `ports`
/// (the room's game port plus the live set of joiner ports — see
/// `lan_filter::CapturePorts`) and forwards each one's payload to
/// `on_packet` (from a plain thread, never a tokio task -- same contract
/// as the Windows capture).
pub fn spawn(
    ports: CapturePorts,
    enabled: Arc<AtomicBool>,
    on_packet: impl Fn(Vec<u8>) + Send + Clone + 'static,
) -> Result<LanCaptureHandle, LanCaptureError> {
    // SOCK_DGRAM ("cooked"): the kernel strips the link-layer header, so
    // each read is the bare IP packet the shared classifier expects.
    let socket = Socket::new(
        Domain::PACKET,
        Type::DGRAM,
        Some(Protocol::from(i32::from(ETH_P_ALL.to_be()))),
    )
    .map_err(LanCaptureError::Socket)?;
    socket
        .set_read_timeout(Some(RECV_TIMEOUT))
        .map_err(LanCaptureError::Socket)?;
    info!(local_ips = ?local_ipv4_addrs(), ports = ?ports.lock().unwrap(), "starting LAN capture on an AF_PACKET socket (all interfaces)");

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let thread = std::thread::spawn(move || run_capture_loop(socket, &ports, &thread_stop, &enabled, on_packet));
    Ok(LanCaptureHandle { stop, _thread: thread })
}

fn run_capture_loop(
    socket: Socket,
    ports: &CapturePorts,
    stop: &AtomicBool,
    enabled: &AtomicBool,
    on_packet: impl Fn(Vec<u8>),
) {
    let mut buffer = vec![MaybeUninit::<u8>::uninit(); RECV_BUF_LEN];
    let mut total_seen: u64 = 0;
    // Interfaces come and go (a container's network attaches late, a VPN
    // connects): refresh which addresses count as ours now and then.
    let mut local_ips = local_ipv4_addrs();
    let mut refreshed = std::time::Instant::now();

    while !stop.load(Ordering::Relaxed) {
        let (len, from) = match socket.recv_from(&mut buffer) {
            Ok(received) => received,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted
                ) =>
            {
                continue
            }
            Err(error) => {
                warn!(%error, "packet capture socket error, ignoring and continuing");
                continue;
            }
        };
        if refreshed.elapsed() > Duration::from_secs(10) {
            local_ips = local_ipv4_addrs();
            refreshed = std::time::Instant::now();
        }
        // Only what this host sent: the same broadcast also shows up as
        // received on other interfaces/loopback, and relaying it twice
        // would duplicate every announcement.
        // The kernel reports the address as long as `sll_addr` actually
        // used (18 bytes for Ethernet), not the whole struct: it only needs
        // to reach `sll_pkttype`.
        let (protocol, pkttype) = if from.len() as usize > std::mem::offset_of!(SockaddrLl, pkttype) {
            // SAFETY: an AF_PACKET socket fills the address as a
            // `sockaddr_ll` (in storage large enough for the whole struct),
            // and we checked it reaches `pkttype`.
            let addr = unsafe { &*from.as_ptr().cast::<SockaddrLl>() };
            (u16::from_be(addr.protocol), addr.pkttype)
        } else {
            continue;
        };
        if pkttype != PACKET_OUTGOING || protocol != ETH_P_IP {
            continue;
        }
        total_seen += 1;
        // SAFETY: `recv_from` initialized the first `len` bytes.
        let packet = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), len) };
        match classify_packet(packet, ports, &local_ips) {
            CaptureOutcome::Relayed(payload) => {
                if !enabled.load(Ordering::Relaxed) {
                    continue;
                }
                debug!(
                    bytes = payload.len(),
                    "captured an outgoing UDP packet to the game port, relaying it"
                );
                on_packet(payload);
            }
            CaptureOutcome::NotOurSource(source) => {
                debug!(%source, "ignored: UDP packet to the game port, but not from one of our own IPs");
            }
            CaptureOutcome::WrongPort(_)
            | CaptureOutcome::NotUdp
            | CaptureOutcome::NotIpv4
            | CaptureOutcome::Unparseable => {}
        }
    }
    info!(total_seen, "capture loop stopped");
}
