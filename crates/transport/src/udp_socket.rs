use crate::error::TransportError;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddr};
use tokio::net::UdpSocket;

/// Thin async wrapper around a UDP socket that is explicitly configured for
/// LAN relay duty: address reuse (so the bridge can restart without waiting
/// out `TIME_WAIT`) and, optionally, `SO_BROADCAST` so the process is allowed
/// to send to the limited broadcast address.
pub struct UdpTransport {
    socket: UdpSocket,
    local_addr: SocketAddr,
}

impl UdpTransport {
    pub async fn bind(addr: SocketAddr, enable_broadcast: bool) -> Result<Self, TransportError> {
        let domain = if addr.is_ipv6() { Domain::IPV6 } else { Domain::IPV4 };

        let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP)).map_err(TransportError::Configure)?;

        socket.set_reuse_address(true).map_err(TransportError::Configure)?;
        #[cfg(unix)]
        socket.set_reuse_port(true).map_err(TransportError::Configure)?;
        socket.set_nonblocking(true).map_err(TransportError::Configure)?;
        if enable_broadcast {
            socket.set_broadcast(true).map_err(TransportError::Configure)?;
        }
        socket
            .bind(&addr.into())
            .map_err(|source| TransportError::Bind { addr, source })?;

        let std_socket: std::net::UdpSocket = socket.into();
        let socket = UdpSocket::from_std(std_socket).map_err(TransportError::Io)?;
        let local_addr = socket.local_addr().map_err(TransportError::Io)?;

        Ok(Self { socket, local_addr })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize, TransportError> {
        self.socket.send_to(buf, target).await.map_err(TransportError::Io)
    }

    pub async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), TransportError> {
        self.socket.recv_from(buf).await.map_err(TransportError::Io)
    }

    /// The IPv4 limited broadcast address on the given port. Works without
    /// needing to know the local subnet's broadcast address in advance.
    pub fn broadcast_addr(port: u16) -> SocketAddr {
        SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)), port)
    }
}
