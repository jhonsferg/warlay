//! UDP sockets for the helper's rendezvous and relay.
//!
//! The relay forwards every datagram of every room through one socket. With
//! the operating system's default buffers (a few hundred KB), a burst from
//! many rooms at once overflows them and the kernel silently drops packets,
//! which the players feel as lag. Bigger buffers absorb the burst.

use std::io;
use std::net::SocketAddr;

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tracing::info;

/// What is asked of the kernel for each direction. The system may grant less
/// (on Linux the ceiling is `net.core.rmem_max` / `wmem_max`).
pub const SOCKET_BUFFER_BYTES: usize = 4 * 1024 * 1024;

/// Binds a UDP socket with enlarged buffers. Asking for them is best effort:
/// a refusal leaves the default size, it is never an error.
pub fn bind_with_buffers(addr: SocketAddr, name: &str) -> io::Result<UdpSocket> {
    let socket = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_nonblocking(true)?;
    let _ = socket.set_recv_buffer_size(SOCKET_BUFFER_BYTES);
    let _ = socket.set_send_buffer_size(SOCKET_BUFFER_BYTES);
    socket.bind(&addr.into())?;
    info!(
        socket = name,
        %addr,
        recv_buffer = socket.recv_buffer_size().unwrap_or(0),
        send_buffer = socket.send_buffer_size().unwrap_or(0),
        "UDP socket bound"
    );
    UdpSocket::from_std(socket.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_socket_binds_and_carries_datagrams() {
        let server = bind_with_buffers("127.0.0.1:0".parse().unwrap(), "test").unwrap();
        let addr = server.local_addr().unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client.send_to(b"ping", addr).await.unwrap();
        let mut buf = [0_u8; 16];
        let (len, _) = server.recv_from(&mut buf).await.unwrap();
        assert_eq!(&buf[..len], b"ping");
    }

    #[tokio::test]
    async fn the_buffers_are_not_smaller_than_the_default() {
        let enlarged = bind_with_buffers("127.0.0.1:0".parse().unwrap(), "test").unwrap();
        let plain = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let size = |s: &UdpSocket| socket2::SockRef::from(s).recv_buffer_size().unwrap();
        assert!(size(&enlarged) >= size(&plain));
    }
}
