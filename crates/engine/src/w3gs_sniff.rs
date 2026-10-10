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
const W3GS_CREATEGAME: u8 = 0x31;

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
}
