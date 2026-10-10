use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use thiserror::Error;

pub const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

#[derive(Debug, Error)]
pub enum CipherError {
    #[error("encryption failed")]
    Encrypt,
    #[error("decryption failed: wrong key, tampered frame, or a replayed/out-of-order counter")]
    Decrypt,
}

/// Encrypts one direction of a session. The nonce is never random — it's
/// this sender's own strictly increasing message counter, so no nonce is
/// ever reused for a given key (the one hard requirement of ChaCha20Poly1305
/// safety) as long as this struct's `counter` isn't reset, which nothing
/// here does.
pub struct Sealer {
    cipher: ChaCha20Poly1305,
    counter: u64,
}

/// Decrypts the matching direction. Rejects any frame whose counter is not
/// strictly greater than the last one accepted, which is what makes replaying
/// a captured frame (or reordering one) fail closed rather than silently
/// re-processing it.
pub struct Opener {
    cipher: ChaCha20Poly1305,
    last_counter: Option<u64>,
}

fn nonce_for_counter(counter: u64) -> [u8; NONCE_LEN] {
    let mut nonce = [0_u8; NONCE_LEN];
    nonce[4..].copy_from_slice(&counter.to_be_bytes());
    nonce
}

impl Sealer {
    pub fn new(key: [u8; KEY_LEN]) -> Self {
        Self {
            cipher: ChaCha20Poly1305::new(Key::from_slice(&key)),
            counter: 0,
        }
    }

    /// Returns `nonce (12 bytes) || ciphertext+tag`, ready to send as-is.
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, CipherError> {
        let nonce_bytes = nonce_for_counter(self.counter);
        self.counter = self.counter.checked_add(1).expect("session exceeded 2^64 messages");

        let mut ciphertext = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
            .map_err(|_| CipherError::Encrypt)?;

        let mut framed = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        framed.extend_from_slice(&nonce_bytes);
        framed.append(&mut ciphertext);
        Ok(framed)
    }
}

impl Opener {
    pub fn new(key: [u8; KEY_LEN]) -> Self {
        Self {
            cipher: ChaCha20Poly1305::new(Key::from_slice(&key)),
            last_counter: None,
        }
    }

    pub fn open(&mut self, framed: &[u8]) -> Result<Vec<u8>, CipherError> {
        if framed.len() < NONCE_LEN {
            return Err(CipherError::Decrypt);
        }
        let (nonce_bytes, ciphertext) = framed.split_at(NONCE_LEN);
        let counter = u64::from_be_bytes(nonce_bytes[4..].try_into().expect("8 bytes"));

        if let Some(last) = self.last_counter {
            if counter <= last {
                return Err(CipherError::Decrypt);
            }
        }

        let plaintext = self
            .cipher
            .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
            .map_err(|_| CipherError::Decrypt)?;

        self.last_counter = Some(counter);
        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_then_open_round_trips() {
        let key = [7_u8; KEY_LEN];
        let mut sealer = Sealer::new(key);
        let mut opener = Opener::new(key);

        let framed = sealer.seal(b"hola sala").unwrap();
        let plaintext = opener.open(&framed).unwrap();
        assert_eq!(plaintext, b"hola sala");
    }

    #[test]
    fn sequential_messages_all_decrypt_in_order() {
        let key = [3_u8; KEY_LEN];
        let mut sealer = Sealer::new(key);
        let mut opener = Opener::new(key);

        for i in 0..5 {
            let msg = format!("mensaje {i}");
            let framed = sealer.seal(msg.as_bytes()).unwrap();
            let plaintext = opener.open(&framed).unwrap();
            assert_eq!(plaintext, msg.as_bytes());
        }
    }

    #[test]
    fn replaying_a_captured_frame_is_rejected() {
        let key = [9_u8; KEY_LEN];
        let mut sealer = Sealer::new(key);
        let mut opener = Opener::new(key);

        let first = sealer.seal(b"first").unwrap();
        opener.open(&first).unwrap();

        let second = sealer.seal(b"second").unwrap();
        opener.open(&second).unwrap();

        // Replaying the first (already-consumed) frame must fail.
        assert!(opener.open(&first).is_err());
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let mut sealer = Sealer::new([1_u8; KEY_LEN]);
        let mut opener = Opener::new([2_u8; KEY_LEN]);

        let framed = sealer.seal(b"secret").unwrap();
        assert!(opener.open(&framed).is_err());
    }
}
