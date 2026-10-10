use hmac::{Hmac, Mac};
use sha2::Sha256;

pub const PROOF_LEN: usize = 32;

type HmacSha256 = Hmac<Sha256>;

/// Proves possession of a previously issued persistent key (`psk`) without
/// ever resending it: an HMAC over both ephemeral public keys from *this*
/// connection's handshake. Binding the proof to those keys (rather than,
/// say, a constant) is what stops a captured proof from being replayed on a
/// different connection — each handshake has fresh, unique ephemeral keys.
pub fn compute_proof(psk: &[u8], client_pubkey: &[u8; 32], server_pubkey: &[u8; 32]) -> [u8; PROOF_LEN] {
    let mut mac = HmacSha256::new_from_slice(psk).expect("HMAC accepts keys of any length");
    mac.update(client_pubkey);
    mac.update(server_pubkey);
    mac.finalize().into_bytes().into()
}

pub fn verify_proof(psk: &[u8], client_pubkey: &[u8; 32], server_pubkey: &[u8; 32], proof: &[u8; PROOF_LEN]) -> bool {
    let expected = compute_proof(psk, client_pubkey, server_pubkey);
    constant_time_eq::constant_time_eq(&expected, proof)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_accepts_a_correctly_computed_proof() {
        let psk = [42_u8; 32];
        let client_pk = [1_u8; 32];
        let server_pk = [2_u8; 32];

        let proof = compute_proof(&psk, &client_pk, &server_pk);
        assert!(verify_proof(&psk, &client_pk, &server_pk, &proof));
    }

    #[test]
    fn verify_rejects_wrong_psk() {
        let client_pk = [1_u8; 32];
        let server_pk = [2_u8; 32];
        let proof = compute_proof(&[42_u8; 32], &client_pk, &server_pk);

        assert!(!verify_proof(&[43_u8; 32], &client_pk, &server_pk, &proof));
    }

    #[test]
    fn verify_rejects_proof_from_a_different_session() {
        let psk = [42_u8; 32];
        let proof = compute_proof(&psk, &[1_u8; 32], &[2_u8; 32]);

        // Same PSK, different ephemeral keys (a different connection).
        assert!(!verify_proof(&psk, &[9_u8; 32], &[8_u8; 32], &proof));
    }
}
