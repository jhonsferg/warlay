//! Time-based one-time passwords (RFC 6238, SHA-1, 6 digits, 30 seconds), the
//! codes authenticator apps show. Plain functions here; the HTTP side is in
//! `routes::totp`.

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const STEP_SECS: u64 = 30;
const DIGITS: u32 = 6;
const SECRET_LEN: usize = 20;
const NONCE_LEN: usize = 12;

/// A new random secret (20 bytes, what RFC 4226 recommends).
pub fn generate_secret() -> [u8; SECRET_LEN] {
    let mut secret = [0_u8; SECRET_LEN];
    getrandom::fill(&mut secret).expect("the OS random number generator is unavailable");
    secret
}

/// The code for one 30-second step.
pub fn code_at(secret: &[u8], step: u64) -> String {
    let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(secret).expect("HMAC takes any key length");
    mac.update(&step.to_be_bytes());
    let hash = mac.finalize().into_bytes();
    let offset = (hash[19] & 0x0f) as usize;
    let binary = (u32::from(hash[offset] & 0x7f) << 24)
        | (u32::from(hash[offset + 1]) << 16)
        | (u32::from(hash[offset + 2]) << 8)
        | u32::from(hash[offset + 3]);
    format!("{:0width$}", binary % 10_u32.pow(DIGITS), width = DIGITS as usize)
}

/// The step `code` is valid for at `now_secs`, allowing one step either way
/// for clock drift, if it is later than `after_step` (a code works once: this
/// is what stops a code seen over the shoulder from being replayed).
pub fn verify(secret: &[u8], code: &str, now_secs: u64, after_step: u64) -> Option<u64> {
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let now = now_secs / STEP_SECS;
    // Every candidate is compared, so timing does not say which one matched.
    let mut found = None;
    for step in now.saturating_sub(1)..=now + 1 {
        let expected = code_at(secret, step);
        if constant_time_eq::constant_time_eq(expected.as_bytes(), code.as_bytes()) && step > after_step {
            found = Some(step);
        }
    }
    found
}

/// RFC 4648 base32 without padding, how authenticator apps take a secret.
pub fn base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let (mut buffer, mut bits) = (0_u32, 0_u32);
    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn encode_component(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

/// What the QR code of an authenticator app holds.
pub fn otpauth_uri(issuer: &str, account: &str, secret: &[u8]) -> String {
    let issuer_part = encode_component(issuer);
    format!(
        "otpauth://totp/{issuer_part}:{}?secret={}&issuer={issuer_part}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECS}",
        encode_component(account),
        base32(secret),
    )
}

/// The key that seals secrets in the database: derived from the server's
/// secret, so it is never stored next to what it protects.
fn key(server_secret: &str) -> Key {
    let digest = Sha256::new()
        .chain_update(b"warlay/totp-at-rest/v1")
        .chain_update(server_secret.as_bytes())
        .finalize();
    *Key::from_slice(&digest)
}

/// `nonce || ciphertext`, bound to the account so a row cannot be moved to
/// another one.
pub fn seal(server_secret: &str, user_id: Uuid, secret: &[u8]) -> Vec<u8> {
    let mut nonce = [0_u8; NONCE_LEN];
    getrandom::fill(&mut nonce).expect("the OS random number generator is unavailable");
    let ciphertext = ChaCha20Poly1305::new(&key(server_secret))
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: secret,
                aad: user_id.as_bytes(),
            },
        )
        .expect("encrypting in memory cannot fail");
    [nonce.as_slice(), &ciphertext].concat()
}

pub fn open(server_secret: &str, user_id: Uuid, sealed: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() <= NONCE_LEN {
        return None;
    }
    let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
    ChaCha20Poly1305::new(&key(server_secret))
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: user_id.as_bytes(),
            },
        )
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 appendix B (SHA-1 secret "12345678901234567890"), 6-digit tail
    // of the 8-digit reference values.
    const RFC_SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn matches_the_rfc_6238_test_vectors() {
        for (time, expected) in [(59, "287082"), (1_111_111_109, "081804"), (1_234_567_890, "005924")] {
            assert_eq!(code_at(RFC_SECRET, time / STEP_SECS), expected, "t={time}");
        }
    }

    #[test]
    fn accepts_one_step_of_drift_and_each_code_once() {
        let now = 1_234_567_890;
        let step = now / STEP_SECS;
        let current = code_at(RFC_SECRET, step);
        assert_eq!(verify(RFC_SECRET, &current, now, 0), Some(step));
        // Typed with a space, as apps display it.
        let spaced = format!("{} {}", &current[..3], &current[3..]);
        assert_eq!(verify(RFC_SECRET, &spaced, now, 0), Some(step));
        // Slightly late or early clocks still work...
        assert_eq!(
            verify(RFC_SECRET, &code_at(RFC_SECRET, step - 1), now, 0),
            Some(step - 1)
        );
        assert_eq!(
            verify(RFC_SECRET, &code_at(RFC_SECRET, step + 1), now, 0),
            Some(step + 1)
        );
        // ...two steps away does not, and a spent step cannot be used again.
        assert_eq!(verify(RFC_SECRET, &code_at(RFC_SECRET, step + 2), now, 0), None);
        assert_eq!(verify(RFC_SECRET, &current, now, step), None);
        assert_eq!(verify(RFC_SECRET, "12345", now, 0), None);
        assert_eq!(verify(RFC_SECRET, "abcdef", now, 0), None);
    }

    #[test]
    fn base32_and_the_otpauth_uri_follow_the_conventions() {
        // RFC 4648 test vectors.
        assert_eq!(base32(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32(b"fooba"), "MZXW6YTB");
        assert_eq!(base32(b"f"), "MY");
        let uri = otpauth_uri("Warlay", "jhon fernandez", RFC_SECRET);
        assert!(uri.starts_with(
            "otpauth://totp/Warlay:jhon%20fernandez?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Warlay"
        ));
        assert!(uri.ends_with("&algorithm=SHA1&digits=6&period=30"));
    }

    #[test]
    fn sealed_secrets_open_only_for_their_account_and_server() {
        let user = Uuid::new_v4();
        let secret = generate_secret();
        let sealed = seal("server-secret-0123456789", user, &secret);
        assert_ne!(&sealed[NONCE_LEN..], secret.as_slice());
        assert_eq!(
            open("server-secret-0123456789", user, &sealed).as_deref(),
            Some(secret.as_slice())
        );
        assert_eq!(open("another-server-secret-0000", user, &sealed), None);
        assert_eq!(open("server-secret-0123456789", Uuid::new_v4(), &sealed), None);
        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert_eq!(open("server-secret-0123456789", user, &tampered), None);
        assert_ne!(
            sealed,
            seal("server-secret-0123456789", user, &secret),
            "a fresh nonce each time"
        );
    }
}
