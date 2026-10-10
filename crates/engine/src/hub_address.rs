use super::NetworkError;
use url::Url;

/// A hub's address exactly as the desktop app stores it, normalized into
/// the WebSocket URL the matchmaking client dials.
///
/// Accepted forms:
/// - `host:port` -- legacy, plain `ws://host:port/ws` (no TLS).
/// - `ws://host[:port][/path]` -- plain, explicit.
/// - `wss://host[:port][/path]` -- TLS; the hub's certificate is verified,
///   and the TCP game tunnel to the same host is TLS-wrapped too.
///
/// A missing path (or a bare `/`) defaults to the hub's `/ws` endpoint.
#[derive(Debug, Clone)]
pub struct HubAddress {
    url: Url,
}

impl HubAddress {
    pub fn parse(address: &str) -> Result<Self, NetworkError> {
        let address = address.trim();
        let invalid = || NetworkError::InvalidHubAddress(address.to_string());

        let with_scheme = if address.contains("://") {
            address.to_string()
        } else {
            format!("ws://{address}")
        };
        let mut url = Url::parse(&with_scheme).map_err(|_| invalid())?;
        if !matches!(url.scheme(), "ws" | "wss") || url.host_str().map_or(true, str::is_empty) {
            return Err(invalid());
        }
        if url.path().is_empty() || url.path() == "/" {
            url.set_path("/ws");
        }
        Ok(Self { url })
    }

    pub fn ws_url(&self) -> &str {
        self.url.as_str()
    }

    /// The bare host (brackets stripped from an IPv6 literal), suitable
    /// for DNS resolution and as the TLS server name.
    pub fn host(&self) -> &str {
        self.url
            .host_str()
            .unwrap_or_default()
            .trim_start_matches('[')
            .trim_end_matches(']')
    }

    pub fn is_secure(&self) -> bool {
        self.url.scheme() == "wss"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_host_port_is_plain_ws() {
        let address = HubAddress::parse("161.132.40.53:7777").unwrap();
        assert_eq!(address.ws_url(), "ws://161.132.40.53:7777/ws");
        assert_eq!(address.host(), "161.132.40.53");
        assert!(!address.is_secure());
    }

    #[test]
    fn wss_without_path_defaults_to_ws_endpoint() {
        let address = HubAddress::parse(" wss://warlay.codesoftlabs.com ").unwrap();
        assert_eq!(address.ws_url(), "wss://warlay.codesoftlabs.com/ws");
        assert_eq!(address.host(), "warlay.codesoftlabs.com");
        assert!(address.is_secure());
    }

    #[test]
    fn explicit_path_and_port_are_kept() {
        let address = HubAddress::parse("wss://hub.example.com:8443/custom").unwrap();
        assert_eq!(address.ws_url(), "wss://hub.example.com:8443/custom");
    }

    #[test]
    fn ipv6_host_is_unbracketed() {
        let address = HubAddress::parse("ws://[::1]:7777").unwrap();
        assert_eq!(address.host(), "::1");
    }

    #[test]
    fn rejects_other_schemes_and_garbage() {
        assert!(HubAddress::parse("https://warlay.codesoftlabs.com").is_err());
        assert!(HubAddress::parse("").is_err());
        assert!(HubAddress::parse("wss://").is_err());
    }
}
