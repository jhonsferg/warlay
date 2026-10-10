use hmac::{Hmac, Mac};
use sha2::Sha256;

const MAC_LEN: usize = 32;

type HmacSha256 = Hmac<Sha256>;

/// Appends an HMAC-SHA256 tag over `payload` keyed with `secret`, producing
/// a self-contained token (`payload || tag`) that anyone holding the same
/// secret can verify without a lookup -- how the main server vouches for a
/// room membership to a helper server it shares a secret with.
pub fn sign_token(secret: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any length");
    mac.update(payload);
    let mut token = payload.to_vec();
    token.extend_from_slice(&mac.finalize().into_bytes());
    token
}

/// Returns the payload of a token produced by `sign_token` with the same
/// `secret`, or `None` if it was forged, altered or truncated.
pub fn verify_token<'a>(secret: &[u8], token: &'a [u8]) -> Option<&'a [u8]> {
    if token.len() < MAC_LEN {
        return None;
    }
    let (payload, tag) = token.split_at(token.len() - MAC_LEN);
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any length");
    mac.update(payload);
    let expected = mac.finalize().into_bytes();
    constant_time_eq::constant_time_eq(&expected, tag).then_some(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_with_the_same_secret() {
        let token = sign_token(b"secret", b"room 42");
        assert_eq!(verify_token(b"secret", &token), Some(&b"room 42"[..]));
    }

    #[test]
    fn rejects_other_secrets_tampering_and_truncation() {
        let mut token = sign_token(b"secret", b"room 42");
        assert_eq!(verify_token(b"other", &token), None);
        assert_eq!(verify_token(b"secret", &token[..10]), None);
        token[0] ^= 1;
        assert_eq!(verify_token(b"secret", &token), None);
    }
}
