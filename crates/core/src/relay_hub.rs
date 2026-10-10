use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use warlay_matchmaking::RoomMemberships;
use warlay_protocol::RelayFrame;
use warlay_transport::UdpTransport;

/// Hub mode's data-plane engine. Unlike LAN-mesh mode's `RelayEngine` (which
/// reflects broadcast traffic between statically configured peers), the hub
/// serves every room from a single shared UDP socket: each `RelayFrame`
/// self-identifies its room via `room_token`, the hub records the sender as
/// a member of that room, and fans the raw payload out — as plain unicast,
/// never a broadcast — to every other current member. Because nothing here
/// re-broadcasts to the sender's own subnet, there is no self-echo risk and
/// therefore no `LoopGuard` needed, unlike LAN-mesh mode.
pub struct RelayHub {
    transport: Arc<UdpTransport>,
    memberships: Arc<RoomMemberships>,
}

pub struct RelayHubHandle {
    pub data_task: JoinHandle<()>,
    pub cleanup_task: JoinHandle<()>,
}

impl RelayHub {
    pub fn new(transport: Arc<UdpTransport>, memberships: Arc<RoomMemberships>) -> Self {
        Self { transport, memberships }
    }

    pub fn spawn(self: Arc<Self>, membership_ttl: Duration) -> RelayHubHandle {
        let data_task = tokio::spawn(self.clone().run_data_loop());
        let cleanup_task = tokio::spawn(self.run_cleanup_loop(membership_ttl));
        RelayHubHandle {
            data_task,
            cleanup_task,
        }
    }

    async fn run_data_loop(self: Arc<Self>) {
        let mut buf = vec![0_u8; 65_535];
        loop {
            let (len, src) = match self.transport.recv_from(&mut buf).await {
                Ok(result) => result,
                Err(error) => {
                    warn!(%error, "failed to receive datagram on relay hub socket");
                    continue;
                }
            };

            let frame = match RelayFrame::decode(&buf[..len]) {
                Ok(frame) => frame,
                Err(error) => {
                    debug!(%error, %src, "ignoring malformed relay frame");
                    continue;
                }
            };

            self.memberships.touch(frame.room_token, src);
            let targets = self.memberships.members_excluding(frame.room_token, src);
            if targets.is_empty() {
                continue;
            }

            let datagram = &buf[..len];
            for target in targets {
                if let Err(error) = self.transport.send_to(datagram, target).await {
                    warn!(%error, %target, room_token = %frame.room_token, "failed to relay frame to room member");
                }
            }
        }
    }

    async fn run_cleanup_loop(self: Arc<Self>, ttl: Duration) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            self.memberships.prune_expired(ttl);
        }
    }
}
