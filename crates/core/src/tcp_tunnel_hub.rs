use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use uuid::Uuid;
use warlay_protocol::TunnelHello;

const MAX_HELLO_FRAME_BYTES: usize = 4096;

struct RoomBackends {
    open_backend_signal: mpsc::UnboundedSender<()>,
    pending_joiners: VecDeque<TcpStream>,
}

/// Carries the real WC3 game session (TCP) between a room's host and its
/// joiners, since the UDP data plane only ever handles the LAN discovery
/// broadcast, never the actual game connection.
///
/// Each logical WC3 TCP connection becomes exactly two real TCP connections
/// meeting here: a `JoinerStream` from the joiner's local relay, paired
/// FIFO with the next `HostBackend` the host opens in response to a
/// signal sent over its long-lived `HostControl` connection. Once paired,
/// this hub does nothing but copy bytes both ways — no protocol awareness
/// of WC3's own traffic is needed here.
pub struct TcpTunnelHub {
    listener: TcpListener,
    rooms: Arc<Mutex<HashMap<Uuid, RoomBackends>>>,
}

pub struct TcpTunnelHubHandle {
    pub accept_task: JoinHandle<()>,
}

impl TcpTunnelHub {
    pub fn new(listener: TcpListener) -> Self {
        Self {
            listener,
            rooms: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn spawn(self: Arc<Self>) -> TcpTunnelHubHandle {
        let accept_task = tokio::spawn(self.run());
        TcpTunnelHubHandle { accept_task }
    }

    async fn run(self: Arc<Self>) {
        loop {
            let (stream, peer_addr) = match self.listener.accept().await {
                Ok(result) => result,
                Err(error) => {
                    warn!(%error, "failed to accept tcp tunnel connection");
                    continue;
                }
            };
            let hub = self.clone();
            tokio::spawn(async move {
                hub.handle_connection(stream, peer_addr).await;
            });
        }
    }

    async fn handle_connection(&self, mut stream: TcpStream, peer_addr: SocketAddr) {
        // Every client-side socket in this tunnel already disables Nagle's
        // algorithm (see `tcp_tunnel_client.rs`'s `set_nodelay(true)` calls)
        // -- this side of the same connections never did. Nagle buffers
        // small writes hoping to coalesce them with more data, which is
        // exactly the wrong tradeoff for WC3's small, frequent command
        // packets: it adds real, measurable latency (tens of ms, compounding
        // across the two server-side hops a joiner's traffic makes --
        // JoinerStream and HostBackend, both meeting here) for no
        // throughput benefit on a link this size.
        let _ = stream.set_nodelay(true);

        let hello = match read_hello(&mut stream).await {
            Ok(hello) => hello,
            Err(error) => {
                debug!(%error, %peer_addr, "failed to read tunnel handshake");
                return;
            }
        };

        match hello {
            TunnelHello::HostControl { room_token } => self.handle_host_control(room_token, stream).await,
            TunnelHello::HostBackend { room_token } => self.handle_host_backend(room_token, stream).await,
            TunnelHello::JoinerStream { room_token } => self.handle_joiner_stream(room_token, stream).await,
        }
    }

    /// The host keeps exactly one of these open for the room's whole
    /// lifetime; it exists purely to carry "open a backend now" signals, one
    /// `0x01` byte per waiting joiner. Reading from it too is only there to
    /// notice when the host disconnects, so the room can be torn down
    /// immediately rather than leaving stale `pending_joiners` around.
    async fn handle_host_control(&self, room_token: Uuid, mut stream: TcpStream) {
        let (tx, mut rx) = mpsc::unbounded_channel::<()>();
        {
            let mut rooms = self.rooms.lock().await;
            rooms.insert(
                room_token,
                RoomBackends {
                    open_backend_signal: tx,
                    pending_joiners: VecDeque::new(),
                },
            );
        }
        info!(%room_token, "tunnel host control channel registered");

        let mut discard = [0_u8; 64];
        loop {
            tokio::select! {
                signal = rx.recv() => {
                    if signal.is_none() {
                        break;
                    }
                    if stream.write_all(&[1]).await.is_err() {
                        break;
                    }
                }
                read_result = stream.read(&mut discard) => {
                    match read_result {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
            }
        }

        self.rooms.lock().await.remove(&room_token);
        info!(%room_token, "tunnel host control channel closed");
    }

    async fn handle_host_backend(&self, room_token: Uuid, mut backend_stream: TcpStream) {
        let joiner_stream = {
            let mut rooms = self.rooms.lock().await;
            rooms
                .get_mut(&room_token)
                .and_then(|room| room.pending_joiners.pop_front())
        };

        let Some(mut joiner_stream) = joiner_stream else {
            debug!(%room_token, "host backend arrived with no waiting joiner, dropping");
            return;
        };

        // This is the actual real WC3 game TCP session getting bridged —
        // the clearest possible confirmation that two devices are truly
        // talking through the tunnel, not just connected to the hub.
        info!(%room_token, "game session bridged: joiner <-> host, relaying real WC3 traffic");
        match tokio::io::copy_bidirectional(&mut joiner_stream, &mut backend_stream).await {
            Ok((joiner_to_host, host_to_joiner)) => {
                info!(%room_token, joiner_to_host, host_to_joiner, "game session ended");
            }
            Err(error) => {
                warn!(%error, %room_token, "game session tunnel error");
            }
        }
    }

    async fn handle_joiner_stream(&self, room_token: Uuid, stream: TcpStream) {
        let mut rooms = self.rooms.lock().await;
        let Some(room) = rooms.get_mut(&room_token) else {
            debug!(%room_token, "joiner stream arrived but no host is registered for this room");
            return;
        };
        room.pending_joiners.push_back(stream);
        let _ = room.open_backend_signal.send(());
    }
}

async fn read_hello(stream: &mut TcpStream) -> std::io::Result<TunnelHello> {
    let mut len_buf = [0_u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_HELLO_FRAME_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "tunnel hello frame too large",
        ));
    }
    let mut buf = vec![0_u8; len];
    stream.read_exact(&mut buf).await?;
    TunnelHello::decode(&buf).map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    async fn send_hello(stream: &mut TcpStream, hello: &TunnelHello) {
        let encoded = hello.encode().expect("encode should succeed");
        stream
            .write_all(&(encoded.len() as u32).to_le_bytes())
            .await
            .expect("write length prefix");
        stream.write_all(&encoded).await.expect("write hello body");
    }

    #[tokio::test]
    async fn full_round_trip_pipes_bytes_both_ways() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind test listener");
        let hub_addr = listener.local_addr().expect("local addr");
        Arc::new(TcpTunnelHub::new(listener)).spawn();

        let room_token = Uuid::new_v4();

        let mut host_control = TcpStream::connect(hub_addr).await.expect("connect host control");
        send_hello(&mut host_control, &TunnelHello::HostControl { room_token }).await;

        // Give the hub a moment to register the room before the joiner connects.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut joiner_stream = TcpStream::connect(hub_addr).await.expect("connect joiner stream");
        send_hello(&mut joiner_stream, &TunnelHello::JoinerStream { room_token }).await;

        let mut signal_byte = [0_u8; 1];
        timeout(Duration::from_secs(2), host_control.read_exact(&mut signal_byte))
            .await
            .expect("timed out waiting for open-backend signal")
            .expect("read signal byte");
        assert_eq!(signal_byte[0], 1);

        let mut host_backend = TcpStream::connect(hub_addr).await.expect("connect host backend");
        send_hello(&mut host_backend, &TunnelHello::HostBackend { room_token }).await;

        joiner_stream
            .write_all(b"hello from joiner")
            .await
            .expect("write from joiner");
        let mut buf = [0_u8; 17];
        timeout(Duration::from_secs(2), host_backend.read_exact(&mut buf))
            .await
            .expect("timed out reading on host backend")
            .expect("read from host backend");
        assert_eq!(&buf, b"hello from joiner");

        host_backend
            .write_all(b"hello from host!!")
            .await
            .expect("write from host");
        let mut buf2 = [0_u8; 17];
        timeout(Duration::from_secs(2), joiner_stream.read_exact(&mut buf2))
            .await
            .expect("timed out reading on joiner stream")
            .expect("read from joiner stream");
        assert_eq!(&buf2, b"hello from host!!");
    }
}
