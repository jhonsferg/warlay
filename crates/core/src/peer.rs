use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeerHandle {
    pub addr: SocketAddr,
    pub label: String,
}

/// Thread-safe registry of the remote bridge instances this node relays
/// traffic to and from. A peer is identified purely by socket address: any
/// datagram arriving from a registered peer address is treated as tunnel
/// traffic rather than local subnet traffic.
#[derive(Debug, Default)]
pub struct PeerRegistry {
    addrs: RwLock<HashSet<SocketAddr>>,
    labels: RwLock<HashMap<SocketAddr, String>>,
}

impl PeerRegistry {
    pub fn new(initial: impl IntoIterator<Item = SocketAddr>) -> Self {
        let registry = Self::default();
        for addr in initial {
            registry.add(addr, addr.to_string());
        }
        registry
    }

    pub fn add(&self, addr: SocketAddr, label: impl Into<String>) {
        self.addrs.write().expect("peer registry lock poisoned").insert(addr);
        self.labels
            .write()
            .expect("peer registry lock poisoned")
            .insert(addr, label.into());
    }

    pub fn remove(&self, addr: &SocketAddr) -> bool {
        self.labels.write().expect("peer registry lock poisoned").remove(addr);
        self.addrs.write().expect("peer registry lock poisoned").remove(addr)
    }

    pub fn contains(&self, addr: &SocketAddr) -> bool {
        self.addrs.read().expect("peer registry lock poisoned").contains(addr)
    }

    pub fn snapshot(&self) -> Vec<PeerHandle> {
        let addrs = self.addrs.read().expect("peer registry lock poisoned");
        let labels = self.labels.read().expect("peer registry lock poisoned");
        addrs
            .iter()
            .map(|addr| PeerHandle {
                addr: *addr,
                label: labels.get(addr).cloned().unwrap_or_else(|| addr.to_string()),
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.addrs.read().expect("peer registry lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_contains_round_trip() {
        let registry = PeerRegistry::default();
        let addr: SocketAddr = "10.0.0.5:6112".parse().unwrap();

        assert!(!registry.contains(&addr));
        registry.add(addr, "phone-bridge-2");
        assert!(registry.contains(&addr));

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].label, "phone-bridge-2");
    }

    #[test]
    fn remove_drops_membership() {
        let addr: SocketAddr = "10.0.0.5:6112".parse().unwrap();
        let registry = PeerRegistry::new([addr]);

        assert!(registry.remove(&addr));
        assert!(!registry.contains(&addr));
        assert!(registry.is_empty());
    }
}
