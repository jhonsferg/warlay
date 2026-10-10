//! Session security primitives shared by the WebSocket control plane's
//! handshake: an X25519 ephemeral key exchange establishes a fresh,
//! forward-secret pair of directional AEAD keys for every connection
//! (`handshake`), and a persistent pre-shared key issued after a client's
//! first successful authentication lets it *prove* its identity on later
//! connections without ever resending a password or the key itself
//! (`proof`). See `HandshakeError`/module docs for what this does and does
//! not defend against.

mod aead;
mod handshake;
mod proof;

pub use aead::{CipherError, Opener, Sealer, KEY_LEN};
pub use handshake::{DirectionalKeys, EphemeralKeypair, HandshakeError, PUBLIC_KEY_LEN};
pub use proof::{compute_proof, verify_proof, PROOF_LEN};

/// Generates a fresh 32-byte pre-shared key for a newly authenticated
/// client, using the OS CSPRNG.
pub fn generate_psk() -> [u8; KEY_LEN] {
    use rand_core::RngCore;
    let mut psk = [0_u8; KEY_LEN];
    rand_core::OsRng.fill_bytes(&mut psk);
    psk
}
