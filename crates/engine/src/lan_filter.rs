//! Which captured packets are relayed: shared by every platform's LAN
//! capture (raw `SIO_RCVALL` socket on Windows, `AF_PACKET` on Linux), so
//! the scoping rules -- and their tests -- are the same everywhere.

use etherparse::{NetSlice, SlicedPacket, TransportSlice};
use std::net::Ipv4Addr;

/// No filtering on the *content* of the packet — whatever WC3 actually put
/// on the wire gets relayed byte for byte, unguessed. The only two
/// boundaries kept are what scope this to "this game's own broadcast
/// traffic" rather than turning the app into an indiscriminate network tap:
/// `game_port` (otherwise we'd relay this machine's entire IP traffic —
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
pub(crate) fn classify_packet(ip_packet: &[u8], game_port: u16, local_ips: &[Ipv4Addr]) -> CaptureOutcome {
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
    if udp.destination_port() != game_port {
        return CaptureOutcome::WrongPort(udp.destination_port());
    }
    CaptureOutcome::Relayed(udp.payload().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use etherparse::PacketBuilder;

    fn build_ip_packet(source_ip: [u8; 4], dest_port: u16, payload: &[u8]) -> Vec<u8> {
        let builder = PacketBuilder::ipv4(source_ip, [255, 255, 255, 255], 1).udp(6112, dest_port);
        let mut packet = Vec::with_capacity(builder.size(payload.len()));
        builder.write(&mut packet, payload).unwrap();
        packet
    }

    #[test]
    fn relays_any_udp_payload_from_one_of_our_own_ips_on_the_right_port_unfiltered() {
        // Content is never inspected — whatever WC3 (or anything else bound
        // to this port on this machine) actually put on the wire goes
        // through byte for byte, W3GS-looking or not.
        let local_ips = vec![Ipv4Addr::new(192, 168, 1, 50)];
        let payload = [0x00_u8, 0x01, 0x02, 0x03];
        let packet = build_ip_packet([192, 168, 1, 50], 6112, &payload);

        match classify_packet(&packet, 6112, &local_ips) {
            CaptureOutcome::Relayed(relayed) => assert_eq!(relayed, payload),
            _other => panic!("expected Relayed, got a different outcome"),
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
            classify_packet(&packet, 6112, &local_ips),
            CaptureOutcome::NotOurSource(_)
        ));
    }

    #[test]
    fn rejects_traffic_to_an_unrelated_port() {
        let local_ips = vec![Ipv4Addr::new(192, 168, 1, 50)];
        let payload = [0xF7_u8, 0x30, 0x00, 0x04];
        let packet = build_ip_packet([192, 168, 1, 50], 53, &payload);

        assert!(matches!(
            classify_packet(&packet, 6112, &local_ips),
            CaptureOutcome::WrongPort(53)
        ));
    }
}
