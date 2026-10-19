use crate::lan_filter::{classify_packet, CaptureOutcome};
use std::mem::size_of;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle as ThreadHandle;
use thiserror::Error;
use tracing::{debug, info, trace, warn};

use crate::subnet::local_ipv4_addrs;
use windows::Win32::Networking::WinSock::{
    bind, closesocket, recv, setsockopt, WSAGetLastError, WSAIoctl, WSASocketW, WSAStartup, AF_INET, IPPROTO_IP,
    RCVALL_ON, SEND_RECV_FLAGS, SIO_RCVALL, SOCKADDR, SOCKADDR_IN, SOCKET, SOCK_RAW, SOL_SOCKET, SO_RCVTIMEO, WSADATA,
    WSAEMSGSIZE, WSAETIMEDOUT,
};

const RECV_TIMEOUT_MS: u32 = 500;
// A raw IP socket in SIO_RCVALL mode sees every IP datagram on the
// interface, not just ones aimed at our game port — including full-size
// reassembled packets well past a typical Ethernet MTU. Too small a buffer
// here doesn't just truncate one read: recv() fails outright with
// WSAEMSGSIZE for that oversized packet, and on Windows a raw socket can end
// up unusable afterwards, which is what was silently killing capture on the
// real (non-virtual) NIC ~30s into every session. 65535 is the max possible
// IP datagram size, so nothing sent on port 6112 can ever exceed it.
const RECV_BUF_LEN: usize = 65535;

#[derive(Debug, Error)]
pub enum LanCaptureError {
    #[error("failed to initialize Winsock (error code {0})")]
    WinsockStartup(i32),
    #[error("no usable network interface could be opened for packet capture (is the app running as Administrator?)")]
    NoUsableDevice,
}

pub struct LanCaptureHandle {
    stop: Arc<AtomicBool>,
    /// Kept only to own the threads; `abort` doesn't join them.
    _threads: Vec<ThreadHandle<()>>,
}

impl LanCaptureHandle {
    /// Signals every capture thread to stop and returns right away; each one
    /// exits within `RECV_TIMEOUT_MS`, when its blocking `recv()` wakes up.
    pub fn abort(self) {
        self.stop.store(true, Ordering::Relaxed);
        // We intentionally do NOT join the threads here to avoid blocking
        // the async runtime if recv() hangs. The threads will naturally exit
        // the next time their recv() loop times out or processes a packet.
    }
}

/// Captures this machine's own outgoing W3GS UDP broadcasts — the real
/// packets Warcraft III's "Red de Área Local" sends when the user creates a
/// game — using a raw IP socket in Windows' own promiscuous receive mode
/// (`SIO_RCVALL`), the same driver-free technique the well-known sniffer
/// RawCap uses, and forwards each one's UDP payload to `on_packet`. Needs
/// only `ws2_32.dll` — part of Windows itself on every install — unlike
/// binding WC3's own discovery port ourselves (see `network::w3gs`'s
/// history for why that conflicts) or capturing via a third-party driver.
///
/// Still needs Administrator privileges: `SIO_RCVALL` is a privileged
/// operation on Windows no matter which capture technique is used.
pub fn spawn(
    game_port: u16,
    enabled: Arc<AtomicBool>,
    on_packet: impl Fn(Vec<u8>) + Send + Clone + 'static,
) -> Result<LanCaptureHandle, LanCaptureError> {
    unsafe {
        let mut wsa_data = WSADATA::default();
        let startup_result = WSAStartup(0x0202, &mut wsa_data);
        if startup_result != 0 {
            return Err(LanCaptureError::WinsockStartup(startup_result));
        }
    }

    let local_ips = local_ipv4_addrs();
    info!(
        ?local_ips,
        game_port, "starting LAN capture: opening a promiscuous socket per local interface"
    );
    let stop = Arc::new(AtomicBool::new(false));
    let mut threads = Vec::new();

    for ip in &local_ips {
        let socket = match open_promiscuous_socket(*ip) {
            Ok(socket) => {
                info!(%ip, "opened a promiscuous capture socket on this interface");
                socket
            }
            Err(error) => {
                warn!(?error, %ip, "could not open a raw capture socket on this interface, skipping");
                continue;
            }
        };

        let local_ips_owned = local_ips.clone();
        let on_packet = on_packet.clone();
        let stop = stop.clone();
        let enabled = enabled.clone();
        let ip = *ip;
        threads.push(std::thread::spawn(move || {
            run_capture_loop(socket, ip, game_port, &local_ips_owned, &stop, &enabled, on_packet)
        }));
    }

    if threads.is_empty() {
        warn!("no interface could be opened for capture at all — every attempt failed above");
        return Err(LanCaptureError::NoUsableDevice);
    }

    Ok(LanCaptureHandle {
        stop,
        _threads: threads,
    })
}

/// Opens a raw IP socket bound to `ip`'s interface and puts it into
/// `SIO_RCVALL` promiscuous mode — it will now see every IP packet that
/// interface handles, including this machine's own outgoing broadcasts
/// (they loop back through the local stack same as any other traffic on
/// the interface).
fn open_promiscuous_socket(ip: Ipv4Addr) -> windows::core::Result<SOCKET> {
    unsafe {
        let socket = WSASocketW(AF_INET.0 as i32, SOCK_RAW.0, IPPROTO_IP.0, None, 0, 0)?;

        let addr = SOCKADDR_IN::from(SocketAddrV4::new(ip, 0));
        let bind_result = bind(
            socket,
            &addr as *const SOCKADDR_IN as *const SOCKADDR,
            size_of::<SOCKADDR_IN>() as i32,
        );
        if bind_result != 0 {
            let error = windows::core::Error::from_win32();
            let _ = closesocket(socket);
            return Err(error);
        }

        let mut bytes_returned = 0u32;
        let enable: u32 = RCVALL_ON.0 as u32;
        let ioctl_result = WSAIoctl(
            socket,
            SIO_RCVALL,
            Some(&enable as *const u32 as *const core::ffi::c_void),
            size_of::<u32>() as u32,
            None,
            0,
            &mut bytes_returned,
            None,
            None,
        );
        if ioctl_result != 0 {
            let error = windows::core::Error::from_win32();
            let _ = closesocket(socket);
            return Err(error);
        }

        // Bounds the blocking recv() below so the capture loop wakes up
        // regularly enough to notice LanCaptureHandle::abort()'s stop flag.
        let timeout_ms = RECV_TIMEOUT_MS;
        let _ = setsockopt(socket, SOL_SOCKET, SO_RCVTIMEO, Some(&timeout_ms.to_ne_bytes()));

        Ok(socket)
    }
}

fn run_capture_loop(
    socket: SOCKET,
    ip: Ipv4Addr,
    game_port: u16,
    local_ips: &[Ipv4Addr],
    stop: &AtomicBool,
    enabled: &AtomicBool,
    on_packet: impl Fn(Vec<u8>),
) {
    let mut buffer = [0_u8; RECV_BUF_LEN];
    let mut total_seen: u64 = 0;
    let mut logged_first_packet = false;

    while !stop.load(Ordering::Relaxed) {
        let received = unsafe { recv(socket, &mut buffer, SEND_RECV_FLAGS(0)) };
        if received < 0 {
            let error = unsafe { WSAGetLastError() };
            if error == WSAETIMEDOUT {
                continue;
            }
            if error == WSAEMSGSIZE {
                // A packet bigger than our buffer showed up — with a 65535
                // buffer that means a full-size IP datagram unrelated to our
                // small W3GS traffic. Drop just this one read and keep the
                // socket alive; it stays perfectly usable for the next recv().
                debug!(?error, %ip, "raw capture socket saw an oversized packet, dropping it and continuing");
                continue;
            }
            // Other errors (like WSAECONNRESET from ICMP port unreachable) shouldn't permanently kill the capture.
            warn!(?error, %ip, "raw capture socket error, ignoring and continuing");
            continue;
        }

        total_seen += 1;
        if !logged_first_packet {
            logged_first_packet = true;
            info!(
                %ip,
                bytes = received,
                "capture socket is alive: received its first raw IP packet on this interface \
                 (proves SIO_RCVALL is actually seeing traffic here, whatever it turns out to be)"
            );
        }

        match classify_packet(&buffer[..received as usize], game_port, local_ips) {
            CaptureOutcome::Relayed(payload) => {
                // Checked per-packet (not just once at spawn time) so
                // toggling this off from the "Agente" panel takes effect
                // immediately for a capture that's already running, and
                // actually cuts the log/relay noise at its source instead
                // of just muting a downstream consumer of it.
                if !enabled.load(Ordering::Relaxed) {
                    continue;
                }
                // Full payload, not just the header — diagnosing the actual
                // join step (e.g. locating the port W3GS_GAMEINFO tells WC3
                // to connect to) needs more than the first few bytes.
                debug!(
                    %ip,
                    bytes = payload.len(),
                    "captured a real UDP packet from us to the game port, forwarding to relay verbatim"
                );
                if tracing::enabled!(tracing::Level::TRACE) {
                    trace!(%ip, hex = %crate::lan_rebroadcast::hex_dump(&payload), "captured packet payload");
                }
                on_packet(payload);
            }
            CaptureOutcome::NotOurSource(source) => {
                debug!(%ip, %source, "ignored: UDP packet to the game port, but not from one of our own IPs");
            }
            CaptureOutcome::WrongPort(port) => {
                debug!(%ip, dest_port = port, game_port, "ignored: UDP packet from us, but to a different port");
            }
            CaptureOutcome::NotUdp | CaptureOutcome::NotIpv4 | CaptureOutcome::Unparseable => {
                // The overwhelming majority of promiscuous traffic (TCP,
                // ARP-adjacent noise, etc.) — logging every one would drown
                // out everything else, so this stays silent even at debug.
            }
        }
    }

    info!(%ip, total_seen, "capture loop stopped");
    unsafe {
        let _ = closesocket(socket);
    }
}
