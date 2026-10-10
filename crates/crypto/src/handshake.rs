use hkdf::Hkdf;
use sha2::Sha256;
use thiserror::Error;
use x25519_dalek::{PublicKey, StaticSecret};

pub const PUBLIC_KEY_LEN: usize = 32;
const SESSION_KEY_LEN: usize = 32;

#[derive(Debug, Error)]
pub enum HandshakeError {
    #[error("failed to derive session keys from the shared secret")]
    Derive,
}

/// A per-connection key pair. Despite the name, this crate uses
/// `x25519_dalek::StaticSecret` rather than `EphemeralSecret` (which
/// enforces single-use at the type level) purely so the struct can expose
/// `&self` methods; the actual ephemeral property comes from the caller
/// generating a fresh one per connection and dropping it once the session
/// keys are derived — nothing here persists a private key to disk.
pub struct EphemeralKeypair {
    secret: StaticSecret,
    pub public_bytes: [u8; PUBLIC_KEY_LEN],
}

/// Two independent AEAD keys, one per direction, so client-to-server and
/// server-to-client traffic never share a nonce space (see `crate::aead`).
pub struct DirectionalKeys {
    pub client_to_server: [u8; SESSION_KEY_LEN],
    pub server_to_client: [u8; SESSION_KEY_LEN],
}

impl EphemeralKeypair {
    pub fn generate() -> Self {
        // Raw OS entropy keeps this independent of rand_core's API churn;
        // x25519-dalek clamps the scalar itself.
        let mut seed = [0_u8; 32];
        getrandom::fill(&mut seed).expect("the OS random number generator is unavailable");
        let secret = StaticSecret::from(seed);
        let public_bytes = PublicKey::from(&secret).to_bytes();
        Self { secret, public_bytes }
    }

    /// Computes the ECDH shared secret against `peer_public_bytes` and
    /// HKDF-expands it into the two directional session keys. Both sides of
    /// a connection call this with each other's public key and end up with
    /// identical `DirectionalKeys`, without the shared secret itself ever
    /// crossing the wire.
    pub fn derive_session_keys(
        &self,
        peer_public_bytes: &[u8; PUBLIC_KEY_LEN],
    ) -> Result<DirectionalKeys, HandshakeError> {
        let peer_public = PublicKey::from(*peer_public_bytes);
        let shared = self.secret.diffie_hellman(&peer_public);

        let hk = Hkdf::<Sha256>::new(None, shared.as_bytes());
        let mut client_to_server = [0_u8; SESSION_KEY_LEN];
        let mut server_to_client = [0_u8; SESSION_KEY_LEN];
        hk.expand(b"warlay session c2s v1", &mut client_to_server)
            .map_err(|_| HandshakeError::Derive)?;
        hk.expand(b"warlay session s2c v1", &mut server_to_client)
            .map_err(|_| HandshakeError::Derive)?;

        Ok(DirectionalKeys {
            client_to_server,
            server_to_client,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sides_derive_identical_directional_keys() {
        let client = EphemeralKeypair::generate();
        let server = EphemeralKeypair::generate();

        let client_keys = client.derive_session_keys(&server.public_bytes).unwrap();
        let server_keys = server.derive_session_keys(&client.public_bytes).unwrap();

        assert_eq!(client_keys.client_to_server, server_keys.client_to_server);
        assert_eq!(client_keys.server_to_client, server_keys.server_to_client);
        assert_ne!(client_keys.client_to_server, client_keys.server_to_client);
    }

    #[test]
    fn different_connections_derive_different_keys() {
        let client_a = EphemeralKeypair::generate();
        let server_a = EphemeralKeypair::generate();
        let keys_a = client_a.derive_session_keys(&server_a.public_bytes).unwrap();

        let client_b = EphemeralKeypair::generate();
        let server_b = EphemeralKeypair::generate();
        let keys_b = client_b.derive_session_keys(&server_b.public_bytes).unwrap();

        assert_ne!(keys_a.client_to_server, keys_b.client_to_server);
    }
}
