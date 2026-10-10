use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::RwLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ObservedEntry {
    pub source_addr: SocketAddr,
    pub first_seen: Instant,
    pub last_seen: Instant,
    pub payload_sample: Vec<u8>,
}

impl ObservedEntry {
    /// Best-effort human-readable label extracted from the raw datagram.
    /// LAN broadcast/discovery payloads for many games embed the game or
    /// host name as a printable ASCII run; we surface the longest such run
    /// without attempting to parse any game-specific binary format.
    pub fn extract_readable_name(&self) -> Option<String> {
        let filtered: Vec<u8> = self
            .payload_sample
            .iter()
            .copied()
            .map(|byte| {
                if byte.is_ascii_graphic() || byte == b' ' {
                    byte
                } else {
                    b'\n'
                }
            })
            .collect();
        let text = String::from_utf8_lossy(&filtered);
        text.split_whitespace()
            .filter(|word| word.len() >= 3)
            .max_by_key(|word| word.len())
            .map(str::to_string)
    }
}

/// TTL cache of recently observed local-subnet broadcast traffic, keyed by
/// the originating socket address. Backs the control-plane `ListGames` query
/// so a client can ask "what game announcements has this bridge seen
/// recently" without needing to join the data plane itself.
#[derive(Debug, Default)]
pub struct GameObservationStore {
    entries: RwLock<HashMap<SocketAddr, ObservedEntry>>,
}

impl GameObservationStore {
    pub fn record(&self, source_addr: SocketAddr, payload: &[u8]) {
        let now = Instant::now();
        let mut entries = self.entries.write().expect("observation store lock poisoned");
        entries
            .entry(source_addr)
            .and_modify(|entry| {
                entry.last_seen = now;
                entry.payload_sample = payload.to_vec();
            })
            .or_insert_with(|| ObservedEntry {
                source_addr,
                first_seen: now,
                last_seen: now,
                payload_sample: payload.to_vec(),
            });
    }

    pub fn prune_expired(&self, ttl: Duration) {
        let now = Instant::now();
        let mut entries = self.entries.write().expect("observation store lock poisoned");
        entries.retain(|_, entry| now.duration_since(entry.last_seen) < ttl);
    }

    pub fn snapshot(&self) -> Vec<ObservedEntry> {
        self.entries
            .read()
            .expect("observation store lock poisoned")
            .values()
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn record_then_snapshot_contains_the_entry() {
        let store = GameObservationStore::default();
        let addr: SocketAddr = "192.168.1.10:6112".parse().unwrap();

        store.record(addr, b"TowersOfDoom game lobby");

        let snapshot = store.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].source_addr, addr);
        assert_eq!(snapshot[0].extract_readable_name().as_deref(), Some("TowersOfDoom"));
    }

    #[test]
    fn prune_expired_removes_stale_entries() {
        let store = GameObservationStore::default();
        let addr: SocketAddr = "192.168.1.10:6112".parse().unwrap();
        store.record(addr, b"payload");

        sleep(Duration::from_millis(30));
        store.prune_expired(Duration::from_millis(10));

        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn repeated_record_updates_last_seen_without_duplicating() {
        let store = GameObservationStore::default();
        let addr: SocketAddr = "192.168.1.10:6112".parse().unwrap();

        store.record(addr, b"first");
        store.record(addr, b"second");

        let snapshot = store.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].payload_sample, b"second");
    }
}
