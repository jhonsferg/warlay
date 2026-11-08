//! The LAN-bridging rules every client applies, host app and headless
//! simulator alike: what to do with a packet captured from our own WC3, and
//! what to do with a packet that arrived from another member before it is
//! replayed onto the local network.
//!
//! Kept free of sockets and of any UI/runtime types so the exact same code
//! runs in the desktop client, in the lab CLI and in unit tests.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use uuid::Uuid;
use warlay_core::LoopGuard;

use crate::lan_filter::CapturePorts;
use crate::w3gs_sniff::{
    contains_create_game, contains_decreate_game, contains_game_info, contains_game_state, contains_refresh_game,
    contains_search_game, rewrite_gameinfo_port,
};

/// Where a peer's rewritten `GAMEINFO` points: one dedicated local port per
/// peer (`GameGatewayHandle::ensure_listener`).
pub trait JoinPorts {
    fn ensure_listener(&self, peer: Uuid) -> impl Future<Output = std::io::Result<u16>> + Send;
}

impl JoinPorts for crate::GameGatewayHandle {
    fn ensure_listener(&self, peer: Uuid) -> impl Future<Output = std::io::Result<u16>> + Send {
        crate::GameGatewayHandle::ensure_listener(self, peer)
    }
}

/// Outcome of looking at a packet our own WC3 put on the wire.
#[derive(Debug, PartialEq, Eq)]
pub enum OwnCapture {
    /// A packet we replayed ourselves, seen again by the promiscuous
    /// capture: never relay it.
    Echo,
    /// Genuine traffic of our own WC3: relay it. `game_ended` is true when
    /// this re-announcement is the signal that the previous game is over.
    Forward { game_ended: bool },
}

#[derive(Clone)]
pub struct LanBridge {
    pub loop_guard: Arc<LoopGuard>,
    /// True from our WC3's own `CREATEGAME`/`REFRESHGAME` until its next
    /// `SEARCHGAME`/`DECREATEGAME`.
    pub own_game_hosting: Arc<AtomicBool>,
    pub game_started: Arc<AtomicBool>,
    pub capture_ports: CapturePorts,
}

impl LanBridge {
    pub fn new(loop_guard: Arc<LoopGuard>, capture_ports: CapturePorts) -> Self {
        Self {
            loop_guard,
            own_game_hosting: Arc::new(AtomicBool::new(false)),
            game_started: Arc::new(AtomicBool::new(false)),
            capture_ports,
        }
    }

    /// Observer-only: reads `bytes`, tracks the host/browse state, and says
    /// whether to relay. The bytes themselves are never modified.
    pub fn classify_own_capture(&self, bytes: &[u8]) -> OwnCapture {
        // Echo check FIRST, so that everything below only ever reads
        // genuinely own-WC3 traffic (an echoed remote CREATEGAME would
        // otherwise be misread as our own and fire a bogus game-ended).
        if self.loop_guard.is_echo(bytes) {
            return OwnCapture::Echo;
        }
        let mut game_ended = false;
        if contains_create_game(bytes) || contains_refresh_game(bytes) {
            self.own_game_hosting.store(true, Ordering::SeqCst);
            game_ended = self.game_started.swap(false, Ordering::SeqCst);
        } else if contains_search_game(bytes) || contains_decreate_game(bytes) {
            self.own_game_hosting.store(false, Ordering::SeqCst);
        }
        OwnCapture::Forward { game_ended }
    }

    /// Decides what to replay onto the local network for a packet received
    /// from `from`. `None` means: don't replay. The returned bytes are what
    /// actually goes onto the LAN (a `GAMEINFO` comes back with its port
    /// rewritten to `from`'s joiner port).
    pub async fn prepare_inbound(
        &self,
        from: Uuid,
        mut bytes: Vec<u8>,
        gateway: Option<&impl JoinPorts>,
    ) -> Option<Vec<u8>> {
        self.loop_guard.mark(&bytes);
        // While our own WC3 hosts, other members' game state is useless to
        // it and a remote GAMEINFO landing mid-create is a state that never
        // occurs on a real LAN. Join traffic (SEARCHGAME) must still flow:
        // a hosting WC3 answers it with the GAMEINFO that lets people join.
        if self.own_game_hosting.load(Ordering::Relaxed) && contains_game_state(&bytes) {
            return None;
        }
        if contains_game_info(&bytes) {
            if let Some(gateway) = gateway {
                match gateway.ensure_listener(from).await {
                    Ok(port) => {
                        {
                            let mut ports = self.capture_ports.lock().unwrap();
                            if !ports.contains(&port) {
                                ports.push(port);
                            }
                        }
                        if rewrite_gameinfo_port(&mut bytes, port) {
                            // What reaches the LAN is the rewritten packet
                            // and the capture sees it come back from our own
                            // IP: it must be recognised as an echo too, or it
                            // is relayed to the mesh and every member
                            // re-points the game at the echoer's port.
                            self.loop_guard.mark(&bytes);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, %from, "could not open the local join port for the peer");
                    }
                }
            }
        }
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    struct FixedPorts(u16);
    impl JoinPorts for FixedPorts {
        fn ensure_listener(&self, _peer: Uuid) -> impl Future<Output = std::io::Result<u16>> + Send {
            let port = self.0;
            async move { Ok(port) }
        }
    }

    const NO_GATEWAY: Option<&FixedPorts> = None;

    fn bridge() -> LanBridge {
        LanBridge::new(
            Arc::new(LoopGuard::new(Duration::from_secs(5))),
            Arc::new(Mutex::new(vec![6112])),
        )
    }

    const SEARCH: [u8; 16] = [
        0xF7, 0x2F, 0x10, 0x00, 0x50, 0x58, 0x33, 0x57, 0x1A, 0, 0, 0, 0, 0, 0, 0,
    ];
    const CREATE: [u8; 8] = [0xF7, 0x31, 0x08, 0x00, 0x50, 0x58, 0x33, 0x57];
    const REFRESH: [u8; 8] = [0xF7, 0x32, 0x08, 0x00, 1, 0, 0, 0];

    fn gameinfo(port: u16) -> Vec<u8> {
        let mut p = vec![0xF7, 0x30, 0x00, 0x00, 0xAA, 0xBB, 0xCC];
        let len = (p.len() + 2) as u16;
        p[2..4].copy_from_slice(&len.to_le_bytes());
        p.extend_from_slice(&port.to_le_bytes());
        p
    }

    #[tokio::test]
    async fn gameinfo_is_rewritten_to_the_peers_joiner_port_and_scoped_for_capture() {
        let b = bridge();
        let out = b
            .prepare_inbound(Uuid::new_v4(), gameinfo(6112), Some(&FixedPorts(40001)))
            .await
            .expect("replayed");
        assert_eq!(&out[out.len() - 2..], &40001u16.to_le_bytes());
        assert!(b.capture_ports.lock().unwrap().contains(&40001));
    }

    #[tokio::test]
    async fn the_rewritten_gameinfo_is_recognised_when_the_capture_sees_it_come_back() {
        // Regression: only the pre-rewrite bytes were marked, so the packet
        // that actually hit the LAN came back as "our own WC3's traffic".
        let b = bridge();
        let out = b
            .prepare_inbound(Uuid::new_v4(), gameinfo(6112), Some(&FixedPorts(40001)))
            .await
            .unwrap();
        assert_eq!(b.classify_own_capture(&out), OwnCapture::Echo);
        // ... and so does the original, unrewritten one.
        assert_eq!(b.classify_own_capture(&gameinfo(6112)), OwnCapture::Echo);
    }

    #[tokio::test]
    async fn remote_game_state_is_not_replayed_while_our_own_wc3_hosts_but_searches_are() {
        let b = bridge();
        assert_eq!(
            b.classify_own_capture(&CREATE),
            OwnCapture::Forward { game_ended: false }
        );
        assert!(b.own_game_hosting.load(Ordering::SeqCst));

        for state in [REFRESH.to_vec(), gameinfo(6112), CREATE.to_vec()] {
            assert!(b.prepare_inbound(Uuid::new_v4(), state, NO_GATEWAY).await.is_none());
        }
        let out = b.prepare_inbound(Uuid::new_v4(), SEARCH.to_vec(), NO_GATEWAY).await;
        assert_eq!(out.as_deref(), Some(&SEARCH[..]));
    }

    #[tokio::test]
    async fn remote_game_state_is_replayed_verbatim_while_browsing() {
        let b = bridge();
        for packet in [REFRESH.to_vec(), CREATE.to_vec()] {
            let out = b.prepare_inbound(Uuid::new_v4(), packet.clone(), NO_GATEWAY).await;
            assert_eq!(out, Some(packet));
        }
    }

    #[test]
    fn own_search_or_decreate_returns_to_browsing() {
        let b = bridge();
        b.classify_own_capture(&CREATE);
        assert!(b.own_game_hosting.load(Ordering::SeqCst));
        b.classify_own_capture(&SEARCH);
        assert!(!b.own_game_hosting.load(Ordering::SeqCst));

        b.classify_own_capture(&REFRESH);
        assert!(b.own_game_hosting.load(Ordering::SeqCst));
        b.classify_own_capture(&[0xF7, 0x33, 0x08, 0x00, 1, 0, 0, 0]);
        assert!(!b.own_game_hosting.load(Ordering::SeqCst));
    }

    #[test]
    fn a_reannounced_game_signals_the_previous_one_ended_exactly_once() {
        let b = bridge();
        assert_eq!(
            b.classify_own_capture(&CREATE),
            OwnCapture::Forward { game_ended: false }
        );
        b.game_started.store(true, Ordering::SeqCst);
        assert_eq!(
            b.classify_own_capture(&REFRESH),
            OwnCapture::Forward { game_ended: true }
        );
        assert_eq!(
            b.classify_own_capture(&REFRESH),
            OwnCapture::Forward { game_ended: false }
        );
    }

    #[tokio::test]
    async fn a_replayed_remote_create_is_not_misread_as_our_own_game() {
        // Without the echo check first, a remote CREATEGAME replayed onto
        // our LAN would flip us to "hosting" and fire a bogus game-ended.
        let b = bridge();
        b.game_started.store(true, Ordering::SeqCst);
        b.prepare_inbound(Uuid::new_v4(), CREATE.to_vec(), NO_GATEWAY).await;
        assert_eq!(b.classify_own_capture(&CREATE), OwnCapture::Echo);
        assert!(!b.own_game_hosting.load(Ordering::SeqCst));
        assert!(b.game_started.load(Ordering::SeqCst));
    }
}
