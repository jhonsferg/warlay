use super::NetworkError;
use std::net::Ipv4Addr;

/// A parsed IPv4 CIDR range with iteration over its usable host addresses.
/// Implemented against `std` only so this module has no third-party CIDR
/// dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Cidr {
    network: Ipv4Addr,
    prefix_len: u8,
}

impl Ipv4Cidr {
    pub fn parse(input: &str) -> Result<Self, NetworkError> {
        let (ip_part, prefix_part) = input
            .split_once('/')
            .ok_or_else(|| NetworkError::InvalidCidr(input.to_string()))?;
        let ip: Ipv4Addr = ip_part
            .parse()
            .map_err(|_| NetworkError::InvalidCidr(input.to_string()))?;
        let prefix_len: u8 = prefix_part
            .parse()
            .map_err(|_| NetworkError::InvalidCidr(input.to_string()))?;
        if prefix_len > 32 {
            return Err(NetworkError::InvalidCidr(input.to_string()));
        }

        let mask = Self::mask_for(prefix_len);
        let network = Ipv4Addr::from(u32::from(ip) & mask);
        Ok(Self { network, prefix_len })
    }

    pub fn from_ip_and_netmask(ip: Ipv4Addr, netmask: Ipv4Addr) -> Self {
        let prefix_len = netmask.octets().iter().map(|byte| byte.count_ones()).sum::<u32>() as u8;
        let mask = Self::mask_for(prefix_len);
        let network = Ipv4Addr::from(u32::from(ip) & mask);
        Self { network, prefix_len }
    }

    fn mask_for(prefix_len: u8) -> u32 {
        if prefix_len == 0 {
            0
        } else {
            u32::MAX << (32 - prefix_len as u32)
        }
    }

    /// Number of usable host addresses (network and broadcast excluded,
    /// except for /31 and /32 where every address is usable).
    pub fn host_count(&self) -> u64 {
        let total = 1u64 << (32 - self.prefix_len as u32);
        if self.prefix_len >= 31 {
            total
        } else {
            total.saturating_sub(2)
        }
    }

    pub fn hosts(&self) -> impl Iterator<Item = Ipv4Addr> {
        let mask = Self::mask_for(self.prefix_len);
        let network_u32 = u32::from(self.network) & mask;
        let broadcast_u32 = network_u32 | !mask;
        let (start, end) = if self.prefix_len >= 31 {
            (network_u32, broadcast_u32)
        } else {
            (network_u32 + 1, broadcast_u32 - 1)
        };
        (start..=end).map(Ipv4Addr::from)
    }
}

impl std::fmt::Display for Ipv4Cidr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.network, self.prefix_len)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedSubnet {
    pub interface_name: String,
    pub ip: String,
    pub netmask: String,
    pub cidr: String,
    pub is_loopback: bool,
}

/// Enumerates every IPv4-capable network interface on this machine: the
/// primary adapter as well as any secondary ones (a phone hotspot uplink, a
/// VPN adapter, a second NIC), so the caller can offer all of them as scan
/// targets.
pub fn list_local_subnets() -> Result<Vec<DetectedSubnet>, NetworkError> {
    let interfaces = if_addrs::get_if_addrs().map_err(NetworkError::InterfaceEnumeration)?;

    Ok(interfaces
        .into_iter()
        .filter_map(|iface| {
            let is_loopback = iface.is_loopback();
            match iface.addr {
                if_addrs::IfAddr::V4(v4) => {
                    let cidr = Ipv4Cidr::from_ip_and_netmask(v4.ip, v4.netmask);
                    Some(DetectedSubnet {
                        interface_name: iface.name,
                        ip: v4.ip.to_string(),
                        netmask: v4.netmask.to_string(),
                        cidr: cidr.to_string(),
                        is_loopback,
                    })
                }
                _ => None,
            }
        })
        .collect())
}

/// Every non-loopback IPv4 address assigned to this machine.
pub fn local_ipv4_addrs() -> Vec<Ipv4Addr> {
    if_addrs::get_if_addrs()
        .map(|interfaces| {
            interfaces
                .into_iter()
                .filter_map(|iface| match iface.addr {
                    if_addrs::IfAddr::V4(v4) if !iface.is_loopback() => Some(v4.ip),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_slash_24_and_enumerates_usable_hosts() {
        let cidr = Ipv4Cidr::parse("192.168.1.0/24").unwrap();
        assert_eq!(cidr.host_count(), 254);
        let hosts: Vec<_> = cidr.hosts().collect();
        assert_eq!(hosts.first(), Some(&Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(hosts.last(), Some(&Ipv4Addr::new(192, 168, 1, 254)));
    }

    #[test]
    fn slash_30_excludes_network_and_broadcast() {
        let cidr = Ipv4Cidr::parse("10.0.0.0/30").unwrap();
        assert_eq!(cidr.host_count(), 2);
        let hosts: Vec<_> = cidr.hosts().collect();
        assert_eq!(hosts, vec![Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(10, 0, 0, 2)]);
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(Ipv4Cidr::parse("not-a-cidr").is_err());
        assert!(Ipv4Cidr::parse("192.168.1.0/33").is_err());
    }

    #[test]
    fn derives_prefix_length_from_netmask() {
        let cidr = Ipv4Cidr::from_ip_and_netmask(Ipv4Addr::new(192, 168, 1, 42), Ipv4Addr::new(255, 255, 255, 0));
        assert_eq!(cidr.to_string(), "192.168.1.0/24");
    }
}
