//! Which captured packets are relayed: shared by every platform's LAN
//! capture (raw `SIO_RCVALL` socket on Windows, `AF_PACKET` on Linux), so
//! the scoping rules -- and their tests -- are the same everywhere.

use etherparse::{NetSlice, SlicedPacket, TransportSlice};
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

/// The UDP destination ports this capture relays. Shared live with the
/// daemon (rather than read once at spawn): the room's game port goes in
/// from the start, and each peer's dedicated local joiner port is appended
/// when that peer's rewritten `GAMEINFO` is replayed locally -- so the
/// capture sees this WC3's own queries toward the exact game it is about
/// to join, on every member, not just the room host.
///
/// A plain `Mutex<Vec<u16>>` on purpose: read from the capture's raw
/// threads (which have no tokio runtime) and written from async tasks.
pub type CapturePorts = Arc<Mutex<Vec<u16>>>;

/// No filtering on the *content* of the packet — whatever WC3 actually put
/// on the wire gets relayed byte for byte, unguessed. The only two
/// boundaries kept are what scope this to "this game's own broadcast
/// traffic" rather than turning the app into an indiscriminate network tap:
/// the port set (otherwise we'd relay this machine's entire IP traffic —
/// browsing, DNS, everything — to the hub) and "source is one of our own
/// IPs" (a promiscuous socket on a shared network can see OTHER devices'
/// traffic too; relaying that would leak a stranger's private packets, or a
/// neighbor's real game, into this Sala).
pub(crate) enum CaptureOutcome {
    Relayed(Vec<u8>),
    NotIpv4,
    NotUdp,
    /// The port is only logged by the Windows capture.
    WrongPort(#[cfg_attr(not(windows), allow(dead_code))] u16),
    NotOurSource(Ipv4Addr),
    Unparseable,
}

/// Classifies a captured raw IP packet, keeping the *reason* it was ignored
/// (not just a plain `None`) so the capture loop can log something more
/// useful than silence when nothing is getting relayed.
pub(crate) fn classify_packet(ip_packet: &[u8], ports: &CapturePorts, local_ips: &[Ipv4Addr]) -> CaptureOutcome {
    let Ok(sliced) = SlicedPacket::from_ip(ip_packet) else {
        return CaptureOutcome::Unparseable;
    };
    let Some(net) = sliced.net else {
        return CaptureOutcome::Unparseable;
    };
    let NetSlice::Ipv4(ipv4) = net else {
        return CaptureOutcome::NotIpv4;
    };
    let Some(TransportSlice::Udp(udp)) = sliced.transport else {
        return CaptureOutcome::NotUdp;
    };

    let source: Ipv4Addr = ipv4.header().source_addr();
    if !local_ips.contains(&source) {
        return CaptureOutcome::NotOurSource(source);
    }
    let destination = udp.destination_port();
    // Locked only for real UDP packets (everything else left above), and
    // only for as long as a `Vec::contains` takes.
    if !ports.lock().unwrap().contains(&destination) {
        return CaptureOutcome::WrongPort(destination);
    }
    CaptureOutcome::Relayed(udp.payload().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use etherparse::PacketBuilder;

    fn ports(list: &[u16]) -> CapturePorts {
        Arc::new(Mutex::new(list.to_vec()))
    }

    fn build_ip_packet(source_ip: [u8; 4], dest_port: u16, payload: &[u8]) -> Vec<u8> {
        let builder = PacketBuilder::ipv4(source_ip, [255, 255, 255, 255], 1).udp(6112, dest_port);
        let mut packet = Vec::with_capacity(builder.size(payload.len()));
        builder.write(&mut packet, payload).unwrap();
        packet
    }

    #[test]
    fn relays_any_udp_payload_from_one_of_our_own_ips_on_a_scoped_port_unfiltered() {
        // Content is never inspected — whatever WC3 (or anything else bound
        // to this port on this machine) actually put on the wire goes
        // through byte for byte, W3GS-looking or not.
        let local_ips = vec![Ipv4Addr::new(192, 168, 1, 50)];
        let payload = [0x00_u8, 0x01, 0x02, 0x03];
        let packet = build_ip_packet([192, 168, 1, 50], 6112, &payload);

        match classify_packet(&packet, &ports(&[6112]), &local_ips) {
            CaptureOutcome::Relayed(relayed) => assert_eq!(relayed, payload),
            _other => panic!("expected Relayed, got a different outcome"),
        }
    }

    #[test]
    fn relays_toward_any_extra_scoped_port_not_just_the_game_port() {
        // A joiner's dedicated local port (advertised through a rewritten
        // GAMEINFO) is just as much "this game's traffic" as 6112: WC3's
        // queries toward the game it is about to join go there.
        let local_ips = vec![Ipv4Addr::new(192, 168, 1, 50)];
        let payload = [0xF7_u8, 0x2F, 0x04, 0x00];
        let packet = build_ip_packet([192, 168, 1, 50], 54321, &payload);

        match classify_packet(&packet, &ports(&[6112, 54321]), &local_ips) {
            CaptureOutcome::Relayed(relayed) => assert_eq!(relayed, payload),
            _other => panic!("expected Relayed for an extra scoped port"),
        }
    }

    #[test]
    fn rejects_a_packet_from_someone_else_on_the_lan() {
        let local_ips = vec![Ipv4Addr::new(192, 168, 1, 50)];
        let payload = [0xF7_u8, 0x30, 0x00, 0x04];
        // A real neighbor's own game (or unrelated private traffic) — we
        // must not leak it into our Sala just because it shares the port.
        let packet = build_ip_packet([192, 168, 1, 99], 6112, &payload);

        assert!(matches!(
            classify_packet(&packet, &ports(&[6112]), &local_ips),
            CaptureOutcome::NotOurSource(_)
        ));
    }

    #[test]
    fn rejects_traffic_to_an_unrelated_port() {
        let local_ips = vec![Ipv4Addr::new(192, 168, 1, 50)];
        let payload = [0xF7_u8, 0x30, 0x00, 0x04];
        let packet = build_ip_packet([192, 168, 1, 50], 53, &payload);

        assert!(matches!(
            classify_packet(&packet, &ports(&[6112, 54321]), &local_ips),
            CaptureOutcome::WrongPort(53)
        ));
    }
}
