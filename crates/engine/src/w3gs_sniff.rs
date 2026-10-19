//! Best-effort detection of specific W3GS in-game protocol messages inside
//! the byte stream a hosting client's tunnel pumps between the real local
//! WC3 process and the hub — see `tcp_tunnel_client::open_host_backend`.
//!
//! "Best-effort" because TCP is a byte stream, not a message stream: a
//! message split across two `read()` calls (plausible over the WAN hop to
//! the hub, even though it's rare on a local read from WC3 itself) won't be
//! caught by scanning one buffer at a time. That's an acceptable
//! degradation here — the UI's manual "Marcar en partida" button remains a
//! guaranteed fallback — but would NOT be acceptable for anything that
//! needs to be reliable (this deliberately never touches or reorders the
//! bytes being proxied, only reads them).
//!
//! The message id below (`0x0A` for `W3GS_COUNTDOWN_START`) comes from the
//! same public W3GS opcode table that already matched this project's own
//! empirically-confirmed captures of `SEARCHGAME`/`GAMEINFO`/`CREATEGAME`/
//! `REFRESHGAME`/`DECREATEGAME` (`0x2F`..`0x33`) — reasonably trustworthy,
//! but worth double-checking against one real hex capture (host clicks
//! "Start Game") the same way those were, since a wrong id just means
//! silent non-detection, never a false positive that could matter.

const W3GS_MAGIC: u8 = 0xF7;
const W3GS_COUNTDOWN_START: u8 = 0x0A;
const W3GS_SEARCHGAME: u8 = 0x2F;
const W3GS_GAMEINFO: u8 = 0x30;
const W3GS_CREATEGAME: u8 = 0x31;
const W3GS_REFRESHGAME: u8 = 0x32;
const W3GS_DECREATEGAME: u8 = 0x33;

/// `W3GS_COUNTDOWN_START` has no payload — just the 4-byte header (magic,
/// message id, 2-byte little-endian total length, which is `4` for an
/// empty body).
pub fn contains_countdown_start(buf: &[u8]) -> bool {
    buf.windows(4)
        .any(|w| w[0] == W3GS_MAGIC && w[1] == W3GS_COUNTDOWN_START && u16::from_le_bytes([w[2], w[3]]) == 4)
}

/// The host re-announcing a game — the only real, observable signal that a
/// previous session is over (W3GS has no "match ended" packet; broadcast
/// traffic for a game just stops once `COUNTDOWN_START` closes its lobby,
/// since actual gameplay uses a different protocol entirely). Unlike
/// `contains_countdown_start`, this doesn't assert an exact declared
/// length — `CREATEGAME`'s payload isn't a fixed empty body, and there's no
/// real capture yet to confirm its exact length, so this matches on
/// magic+id only. Same "worth double-checking against a real hex capture"
/// caveat as the rest of this module.
pub fn contains_create_game(buf: &[u8]) -> bool {
    buf.windows(2).any(|w| w[0] == W3GS_MAGIC && w[1] == W3GS_CREATEGAME)
}

/// A browsing client announcing it wants to see the games on the LAN.
/// Together with `contains_decreate_game`, this is the *browser-side*
/// half of the two-way state the daemon keeps about this machine's own
/// WC3 (see the daemon's `own_game_hosting` docs): a WC3 screen that
/// searches is browsing, never hosting.
pub fn contains_search_game(buf: &[u8]) -> bool {
    buf.windows(2).any(|w| w[0] == W3GS_MAGIC && w[1] == W3GS_SEARCHGAME)
}

/// The host refreshing its open game's announcement (observed in real
/// captures of a hosting session alongside `CREATEGAME`/`DECREATEGAME`),
/// so it is host-side state too: a hosting WC3 re-announces, it never
/// searches.
pub fn contains_refresh_game(buf: &[u8]) -> bool {
    buf.windows(2).any(|w| w[0] == W3GS_MAGIC && w[1] == W3GS_REFRESHGAME)
}

/// The host announcing its game is gone (user left the lobby, the match
/// ended). Closes the "hosting" state again: back to the LAN browser.
pub fn contains_decreate_game(buf: &[u8]) -> bool {
    buf.windows(2).any(|w| w[0] == W3GS_MAGIC && w[1] == W3GS_DECREATEGAME)
}

/// The details packet a host sends in answer to a `SEARCHGAME` — the one
/// carrying the game's name and the port to connect to on join.
pub fn contains_game_info(buf: &[u8]) -> bool {
    buf.windows(2).any(|w| w[0] == W3GS_MAGIC && w[1] == W3GS_GAMEINFO)
}

/// True for the W3GS messages that *describe a hosted game*: the lifecycle
/// broadcasts (`CREATEGAME`/`REFRESHGAME`/`DECREATEGAME`) and `GAMEINFO`.
///
/// These are exactly what the daemon stops replaying onto this machine's
/// local network while **this machine's own WC3 is hosting** (see the
/// daemon's `own_game_hosting`): while we're in our own lobby we can't
/// join anything (so the remote games are useless to us), and a remote
/// `GAMEINFO` landing on the socket right as our own WC3 runs its create
/// handshake is the one packet that never occurs on a real LAN in that
/// state — the prime suspect for WC3's "hubo un fallo" creation error.
/// Everything else (notably `SEARCHGAME`, the join requests a hosting
/// WC3 must keep receiving to answer them with `GAMEINFO`) is still
/// replayed.
pub fn contains_game_state(buf: &[u8]) -> bool {
    contains_create_game(buf)
        || contains_refresh_game(buf)
        || contains_decreate_game(buf)
        || contains_game_info(buf)
}

/// Overwrites the port at the tail of a `W3GS_GAMEINFO` payload (its last
/// two bytes, little-endian) with `port`, so the client that receives it
/// connects to our local tunnel port instead of the sender's real one.
/// Returns whether `packet` was a `GAMEINFO` long enough to rewrite.
pub fn rewrite_gameinfo_port(packet: &mut [u8], port: u16) -> bool {
    if packet.len() >= 4 && packet[0] == W3GS_MAGIC && packet[1] == W3GS_GAMEINFO {
        let len = packet.len();
        let port_bytes = port.to_le_bytes();
        packet[len - 2] = port_bytes[0];
        packet[len - 1] = port_bytes[1];
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_a_countdown_start_header_anywhere_in_the_buffer() {
        let mut buf = vec![1, 2, 3];
        buf.extend_from_slice(&[0xF7, 0x0A, 0x04, 0x00]);
        buf.extend_from_slice(&[9, 9]);
        assert!(contains_countdown_start(&buf));
    }

    #[test]
    fn ignores_unrelated_w3gs_messages() {
        // A real REFRESHGAME-shaped header (0x32), not COUNTDOWN_START.
        let buf = [0xF7, 0x32, 0x08, 0x00, 1, 2, 3, 4];
        assert!(!contains_countdown_start(&buf));
    }

    #[test]
    fn rejects_a_magic_and_id_match_with_the_wrong_declared_length() {
        // Same first two bytes, but not really a COUNTDOWN_START header —
        // don't false-positive just because the id byte happens to match.
        let buf = [0xF7, 0x0A, 0x05, 0x00, 1];
        assert!(!contains_countdown_start(&buf));
    }

    #[test]
    fn empty_and_short_buffers_never_panic() {
        assert!(!contains_countdown_start(&[]));
        assert!(!contains_countdown_start(&[0xF7, 0x0A]));
    }

    #[test]
    fn detects_a_create_game_header_anywhere_in_the_buffer() {
        let mut buf = vec![1, 2, 3];
        buf.extend_from_slice(&[0xF7, 0x31, 0x10, 0x00, 1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(contains_create_game(&buf));
    }

    #[test]
    fn ignores_unrelated_w3gs_messages_for_create_game() {
        // A real COUNTDOWN_START header (0x0A), not CREATEGAME.
        let buf = [0xF7, 0x0A, 0x04, 0x00];
        assert!(!contains_create_game(&buf));
    }

    #[test]
    fn empty_and_short_buffers_never_panic_for_create_game() {
        assert!(!contains_create_game(&[]));
        assert!(!contains_create_game(&[0xF7]));
    }

    #[test]
    fn detects_each_lifecycle_message_by_magic_and_id() {
        let search = [0xF7, 0x2F, 0x04, 0x00, 1, 2];
        let gameinfo = [0xF7, 0x30, 0x08, 0x00, 1, 2, 3, 4];
        let refresh = [0xF7, 0x32, 0x04, 0x00];
        let decreate = [0xF7, 0x33, 0x04, 0x00];

        assert!(contains_search_game(&search));
        assert!(contains_game_info(&gameinfo));
        assert!(contains_refresh_game(&refresh));
        assert!(contains_decreate_game(&decreate));

        assert!(!contains_game_info(&search));
        assert!(!contains_search_game(&gameinfo));
        assert!(!contains_game_state(&search), "SEARCHGAME must stay replayable while hosting");
        assert!(contains_game_state(&gameinfo));
        assert!(contains_game_state(&refresh));
        assert!(contains_decreate_game(&refresh) == false);
    }

    #[test]
    fn finds_a_lifecycle_message_inside_a_larger_buffer() {
        let mut buf = vec![0xAA; 8];
        buf.extend_from_slice(&[0xF7, 0x33, 0x04, 0x00]);
        assert!(contains_decreate_game(&buf));
        assert!(contains_game_state(&buf));
    }

    #[test]
    fn rewrites_only_a_gameinfo_tail_port() {
        let mut gameinfo = [0xF7, 0x30, 0x00, 0x00, 0x15, 0x00]; // port 0x0015 = 21
        assert!(rewrite_gameinfo_port(&mut gameinfo, 5000));
        assert_eq!(&gameinfo[4..], &5000u16.to_le_bytes());

        let mut search = [0xF7, 0x2F, 0x04, 0x00];
        assert!(!rewrite_gameinfo_port(&mut search, 5000));
        assert_eq!(search, [0xF7, 0x2F, 0x04, 0x00], "non-GAMEINFO bytes must be untouched");

        let mut tiny = [0xF7, 0x30, 0x00];
        assert!(!rewrite_gameinfo_port(&mut tiny, 5000));
    }
}
