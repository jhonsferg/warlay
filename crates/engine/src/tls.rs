//! TLS client configuration shared by every secure connection to a hub:
//! the `wss://` matchmaking control plane and the TLS-wrapped TCP game
//! tunnel. The hub's certificate is verified against the Mozilla root set
//! bundled by `webpki-roots`, so a hub behind a public CA (e.g. Let's
//! Encrypt on a reverse proxy) works without any extra setup.
//!
//! Unlike `p2p_tunnel`'s QUIC config, this one never skips verification:
//! a hub has a stable hostname and a real certificate, peers don't.

use rustls::ClientConfig;
use std::sync::{Arc, OnceLock};

/// Built once per process and shared -- cloning the `Arc` is cheap and
/// `ClientConfig` keeps its session-resumption cache across connections.
pub(crate) fn client_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let config = ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .expect("ring supports rustls' default protocol versions")
                .with_root_certificates(roots)
                .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}
