use std::collections::hash_map::DefaultHasher;
use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::time::{Duration, Instant};

type SeenState = (HashSet<u64>, VecDeque<(u64, Instant)>);

/// Suppresses relay feedback loops.
///
/// A bridge injects peer traffic back into its own local subnet by sending a
/// UDP broadcast on the very same port it also listens on for locally
/// originated traffic. The OS delivers that self-sent broadcast back to the
/// bridge's own receiving socket, which would otherwise look identical to a
/// brand-new local packet and get re-forwarded to every peer forever. Instead
/// of trying to distinguish "self" from "other" at the socket layer (not
/// possible for plain UDP broadcast), we fingerprint payloads we are about to
/// inject and drop anything matching that fingerprint for a short TTL.
pub struct LoopGuard {
    ttl: Duration,
    seen: Mutex<SeenState>,
}

impl LoopGuard {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            seen: Mutex::new((HashSet::new(), VecDeque::new())),
        }
    }

    fn hash_of(payload: &[u8]) -> u64 {
        let mut hasher = DefaultHasher::new();
        payload.hash(&mut hasher);
        hasher.finish()
    }

    pub fn mark(&self, payload: &[u8]) {
        let hash = Self::hash_of(payload);
        let expiry = Instant::now() + self.ttl;
        let mut guard = self.seen.lock().expect("loop guard lock poisoned");
        let (set, queue) = &mut *guard;
        if set.insert(hash) {
            queue.push_back((hash, expiry));
        }
    }

    pub fn is_echo(&self, payload: &[u8]) -> bool {
        let hash = Self::hash_of(payload);
        let mut guard = self.seen.lock().expect("loop guard lock poisoned");
        self.prune_locked(&mut guard);
        guard.0.contains(&hash)
    }

    fn prune_locked(&self, guard: &mut SeenState) {
        let now = Instant::now();
        let (set, queue) = guard;
        while let Some((hash, expiry)) = queue.front() {
            if *expiry > now {
                break;
            }
            set.remove(hash);
            queue.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn marked_payload_is_reported_as_echo() {
        let guard = LoopGuard::new(Duration::from_secs(5));
        let payload = b"gameinfo-packet".to_vec();

        assert!(!guard.is_echo(&payload));
        guard.mark(&payload);
        assert!(guard.is_echo(&payload));
    }

    #[test]
    fn unrelated_payload_is_not_an_echo() {
        let guard = LoopGuard::new(Duration::from_secs(5));
        guard.mark(b"packet-a");

        assert!(!guard.is_echo(b"packet-b"));
    }

    #[test]
    fn expired_fingerprint_stops_being_reported_as_echo() {
        let guard = LoopGuard::new(Duration::from_millis(20));
        let payload = b"short-lived".to_vec();

        guard.mark(&payload);
        assert!(guard.is_echo(&payload));

        sleep(Duration::from_millis(40));
        assert!(!guard.is_echo(&payload));
    }
}
