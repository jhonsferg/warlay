//! End-to-end encryption of direct messages: the server stores and relays
//! text it cannot read.
//!
//! # Design
//!
//! Every device has one long-term X25519 identity key. To send a message,
//! the sender picks a fresh ephemeral key and derives a one-off message key
//! from two Diffie-Hellman results (an X3DH without prekeys):
//!
//! - `DH(ephemeral, recipient identity)` -- only the recipient can compute
//!   it, and it is new for every message;
//! - `DH(sender identity, recipient identity)` -- only the two parties can
//!   compute it, so a valid message also proves who sent it.
//!
//! The key goes through HKDF-SHA256 (salted with the three public keys) into
//! XChaCha20-Poly1305. The ids of both users and the client's request id are
//! authenticated as associated data, so a ciphertext can't be replayed as a
//! different message or into another conversation.
//!
//! Each envelope carries **two boxes** of the same plaintext: one for the
//! recipient and one for the sender, so a user can read their own history.
//!
//! # What this does and does not give
//!
//! - Confidentiality and integrity against the server and the network;
//!   authenticity of the sender (deniable: both parties can compute the
//!   shared secret).
//! - Messages are independent: none depends on another's state, so loss or
//!   reordering can never make a conversation undecryptable, and offline
//!   delivery just works.
//! - **No forward secrecy against a stolen identity key**: whoever gets a
//!   device's private key can read the messages that device received or
//!   sent. (A Double Ratchet would add that, at the price of per-peer state
//!   that can desynchronise.)
//! - Keys are per device and there is no key backup: a new device can't read
//!   old messages.
//! - The server still sees metadata: who talks to whom, when, and sizes.
//! - Public keys are trusted on first use; compare [`safety_number`]s out of
//!   band to rule out a substituting server.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use thiserror::Error;
use x25519_dalek::{PublicKey, StaticSecret};

/// Prefix of an encrypted message body, so servers and clients can tell it
/// from plain text.
pub const ENVELOPE_PREFIX: &str = "e2e1:";
/// Longest plaintext accepted (2000 characters of up to 4 bytes each).
pub const MAX_PLAINTEXT: usize = 8000;

const VERSION: u8 = 1;
const INFO: &[u8] = b"warlay-e2e-v1";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const MAX_REQUEST_ID: usize = 64;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum E2eError {
    #[error("the envelope is malformed")]
    Malformed,
    #[error("cannot decrypt: wrong key, tampered, or not meant for this conversation")]
    Decrypt,
    #[error("the public key is not usable")]
    BadKey,
    #[error("the message is too large")]
    TooLarge,
}

/// A device's long-term key pair.
pub struct Identity {
    secret: StaticSecret,
    public: [u8; KEY_LEN],
}

impl Identity {
    pub fn generate() -> Self {
        let mut seed = [0_u8; KEY_LEN];
        getrandom::fill(&mut seed).expect("the OS random number generator is unavailable");
        Self::from_secret(seed)
    }

    pub fn from_secret(secret: [u8; KEY_LEN]) -> Self {
        let secret = StaticSecret::from(secret);
        let public = PublicKey::from(&secret).to_bytes();
        Self { secret, public }
    }

    pub fn secret_bytes(&self) -> [u8; KEY_LEN] {
        self.secret.to_bytes()
    }

    pub fn public_bytes(&self) -> [u8; KEY_LEN] {
        self.public
    }
}

/// A public key as the text exchanged with the server (base64url).
pub fn encode_public_key(key: &[u8; KEY_LEN]) -> String {
    URL_SAFE_NO_PAD.encode(key)
}

/// The inverse of [`encode_public_key`]; `None` for anything that isn't a
/// 32-byte key.
pub fn decode_public_key(text: &str) -> Option<[u8; KEY_LEN]> {
    URL_SAFE_NO_PAD.decode(text).ok()?.try_into().ok()
}

/// Everything about a message that is authenticated but not secret.
struct Binding<'a> {
    sender: [u8; 16],
    recipient: [u8; 16],
    request_id: &'a str,
}

impl Binding<'_> {
    fn aad(&self) -> Vec<u8> {
        let mut aad = Vec::with_capacity(INFO.len() + 32 + self.request_id.len());
        aad.extend_from_slice(INFO);
        aad.extend_from_slice(&self.sender);
        aad.extend_from_slice(&self.recipient);
        aad.extend_from_slice(self.request_id.as_bytes());
        aad
    }
}

fn dh(secret: &StaticSecret, public: &[u8; KEY_LEN]) -> Result<[u8; KEY_LEN], E2eError> {
    let shared = secret.diffie_hellman(&PublicKey::from(*public));
    // A low-order public key makes the result all zeros: refuse it.
    if !shared.was_contributory() {
        return Err(E2eError::BadKey);
    }
    Ok(shared.to_bytes())
}

fn message_key(
    dh_ephemeral: &[u8; KEY_LEN],
    dh_identity: &[u8; KEY_LEN],
    ephemeral_public: &[u8; KEY_LEN],
    sender_public: &[u8; KEY_LEN],
    recipient_public: &[u8; KEY_LEN],
) -> [u8; KEY_LEN] {
    let mut ikm = [0_u8; KEY_LEN * 2];
    ikm[..KEY_LEN].copy_from_slice(dh_ephemeral);
    ikm[KEY_LEN..].copy_from_slice(dh_identity);
    let mut salt = [0_u8; KEY_LEN * 3];
    salt[..KEY_LEN].copy_from_slice(ephemeral_public);
    salt[KEY_LEN..KEY_LEN * 2].copy_from_slice(sender_public);
    salt[KEY_LEN * 2..].copy_from_slice(recipient_public);
    let mut key = [0_u8; KEY_LEN];
    Hkdf::<Sha256>::new(Some(&salt), &ikm)
        .expand(INFO, &mut key)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    key
}

/// `ephemeral public || nonce || ciphertext`.
fn seal_box(
    sender: &Identity,
    recipient_public: &[u8; KEY_LEN],
    binding: &Binding<'_>,
    plaintext: &[u8],
) -> Result<Vec<u8>, E2eError> {
    let mut seed = [0_u8; KEY_LEN];
    getrandom::fill(&mut seed).expect("the OS random number generator is unavailable");
    let ephemeral = StaticSecret::from(seed);
    let ephemeral_public = PublicKey::from(&ephemeral).to_bytes();
    let key = message_key(
        &dh(&ephemeral, recipient_public)?,
        &dh(&sender.secret, recipient_public)?,
        &ephemeral_public,
        &sender.public,
        recipient_public,
    );
    let mut nonce = [0_u8; NONCE_LEN];
    getrandom::fill(&mut nonce).expect("the OS random number generator is unavailable");
    let ciphertext = XChaCha20Poly1305::new((&key).into())
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &binding.aad(),
            },
        )
        .map_err(|_| E2eError::Decrypt)?;
    let mut out = Vec::with_capacity(KEY_LEN + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&ephemeral_public);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Opens a box addressed to `me`, sealed by the owner of `sender_public`
/// (which is `me` for the copy a sender keeps for themselves).
fn open_box(
    me: &Identity,
    sender_public: &[u8; KEY_LEN],
    binding: &Binding<'_>,
    boxed: &[u8],
) -> Result<Vec<u8>, E2eError> {
    if boxed.len() < KEY_LEN + NONCE_LEN + TAG_LEN {
        return Err(E2eError::Malformed);
    }
    let (ephemeral_public, rest) = boxed.split_at(KEY_LEN);
    let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
    let ephemeral_public: [u8; KEY_LEN] = ephemeral_public.try_into().map_err(|_| E2eError::Malformed)?;
    let key = message_key(
        &dh(&me.secret, &ephemeral_public)?,
        &dh(&me.secret, sender_public)?,
        &ephemeral_public,
        sender_public,
        &me.public,
    );
    XChaCha20Poly1305::new((&key).into())
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: &binding.aad(),
            },
        )
        .map_err(|_| E2eError::Decrypt)
}

/// A decrypted direct message.
#[derive(Debug, PartialEq, Eq)]
pub struct Decrypted {
    pub plaintext: Vec<u8>,
    /// The id the sender gave this send; a repeated one is a replay.
    pub request_id: String,
}

/// Encrypts `plaintext` from `me` (user `sender_id`) to the owner of
/// `peer_public` (user `recipient_id`). The result is the message body to
/// post; `request_id` should be unique per send.
pub fn encrypt_dm(
    me: &Identity,
    peer_public: &[u8; KEY_LEN],
    sender_id: [u8; 16],
    recipient_id: [u8; 16],
    request_id: &str,
    plaintext: &[u8],
) -> Result<String, E2eError> {
    if plaintext.len() > MAX_PLAINTEXT || request_id.len() > MAX_REQUEST_ID {
        return Err(E2eError::TooLarge);
    }
    let binding = Binding {
        sender: sender_id,
        recipient: recipient_id,
        request_id,
    };
    let for_peer = seal_box(me, peer_public, &binding, plaintext)?;
    let for_me = seal_box(me, &me.public, &binding, plaintext)?;

    let mut raw = Vec::with_capacity(8 + request_id.len() + for_peer.len() + for_me.len());
    raw.push(VERSION);
    raw.push(request_id.len() as u8);
    raw.extend_from_slice(request_id.as_bytes());
    raw.extend_from_slice(&(for_peer.len() as u32).to_be_bytes());
    raw.extend_from_slice(&for_peer);
    raw.extend_from_slice(&for_me);
    Ok(format!("{ENVELOPE_PREFIX}{}", URL_SAFE_NO_PAD.encode(raw)))
}

/// Decrypts a message body. `peer_public` is the other party's key;
/// `i_am_sender` says which of the two boxes is ours.
pub fn decrypt_dm(
    me: &Identity,
    peer_public: &[u8; KEY_LEN],
    sender_id: [u8; 16],
    recipient_id: [u8; 16],
    envelope: &str,
    i_am_sender: bool,
) -> Result<Decrypted, E2eError> {
    let raw = envelope
        .strip_prefix(ENVELOPE_PREFIX)
        .and_then(|b64| URL_SAFE_NO_PAD.decode(b64).ok())
        .ok_or(E2eError::Malformed)?;
    let [version, id_len, rest @ ..] = raw.as_slice() else {
        return Err(E2eError::Malformed);
    };
    let id_len = *id_len as usize;
    if *version != VERSION || id_len > MAX_REQUEST_ID || rest.len() < id_len + 4 {
        return Err(E2eError::Malformed);
    }
    let (request_id, rest) = rest.split_at(id_len);
    let request_id = std::str::from_utf8(request_id).map_err(|_| E2eError::Malformed)?;
    let (peer_len, boxes) = rest.split_at(4);
    let peer_len = u32::from_be_bytes(peer_len.try_into().map_err(|_| E2eError::Malformed)?) as usize;
    if boxes.len() < peer_len {
        return Err(E2eError::Malformed);
    }
    let (for_peer, for_me) = boxes.split_at(peer_len);
    let binding = Binding {
        sender: sender_id,
        recipient: recipient_id,
        request_id,
    };
    let plaintext = if i_am_sender {
        open_box(me, &me.public, &binding, for_me)?
    } else {
        // The recipient's box: the peer sealed it with their identity key.
        open_box(me, peer_public, &binding, for_peer)?
    };
    Ok(Decrypted {
        plaintext,
        request_id: request_id.to_string(),
    })
}

/// Whether `body` has the shape of an envelope -- the prefix followed by
/// base64url only -- without opening it. For servers, which can't read one
/// but may insist a body that claims to be one really looks like it.
pub fn is_envelope(body: &str) -> bool {
    body.strip_prefix(ENVELOPE_PREFIX).is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    })
}

/// A code two users can compare out of band (in person, by call) to be sure
/// no one -- the server included -- swapped a key: 30 digits in six groups,
/// identical on both sides.
pub fn safety_number(a: &[u8; KEY_LEN], b: &[u8; KEY_LEN]) -> String {
    let (low, high) = if a <= b { (a, b) } else { (b, a) };
    let mut hasher = Sha256::new();
    hasher.update(b"warlay-safety-number-v1");
    hasher.update(low);
    hasher.update(high);
    let digest = hasher.finalize();
    digest[..30]
        .chunks(5)
        .map(|chunk| {
            let n = chunk.iter().fold(0_u64, |acc, byte| (acc << 8) | u64::from(*byte));
            format!("{:05}", n % 100_000)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALICE: [u8; 16] = [1; 16];
    const BOB: [u8; 16] = [2; 16];

    fn pair() -> (Identity, Identity) {
        (Identity::generate(), Identity::generate())
    }

    #[test]
    fn the_recipient_and_the_sender_can_both_read_it() {
        let (alice, bob) = pair();
        let envelope = encrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, "req-1", "hola 👋".as_bytes()).unwrap();
        assert!(envelope.starts_with(ENVELOPE_PREFIX));
        assert!(!envelope.contains("hola"));

        let seen_by_bob = decrypt_dm(&bob, &alice.public_bytes(), ALICE, BOB, &envelope, false).unwrap();
        assert_eq!(seen_by_bob.plaintext, "hola 👋".as_bytes());
        assert_eq!(seen_by_bob.request_id, "req-1");
        let seen_by_alice = decrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, &envelope, true).unwrap();
        assert_eq!(seen_by_alice.plaintext, "hola 👋".as_bytes());
    }

    #[test]
    fn nobody_else_can_read_it() {
        let (alice, bob) = pair();
        let eve = Identity::generate();
        let envelope = encrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, "r", b"secreto").unwrap();
        // Eve, holding her own key, whoever she claims the peer is.
        for peer in [alice.public_bytes(), bob.public_bytes(), eve.public_bytes()] {
            assert!(decrypt_dm(&eve, &peer, ALICE, BOB, &envelope, false).is_err());
            assert!(decrypt_dm(&eve, &peer, ALICE, BOB, &envelope, true).is_err());
        }
        // Bob can't open Alice's own copy, nor Alice Bob's.
        assert!(decrypt_dm(&bob, &alice.public_bytes(), ALICE, BOB, &envelope, true).is_err());
        assert!(decrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, &envelope, false).is_err());
    }

    #[test]
    fn a_substituted_sender_key_is_detected() {
        // The server swaps Alice's key for its own when Bob looks it up.
        let (alice, bob) = pair();
        let mallory = Identity::generate();
        let envelope = encrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, "r", b"hola").unwrap();
        assert_eq!(
            decrypt_dm(&bob, &mallory.public_bytes(), ALICE, BOB, &envelope, false),
            Err(E2eError::Decrypt)
        );
    }

    #[test]
    fn a_message_cannot_be_moved_to_another_conversation() {
        let (alice, bob) = pair();
        let envelope = encrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, "r1", b"hola").unwrap();
        // Another pair of users, or another request id, fails authentication.
        assert!(decrypt_dm(&bob, &alice.public_bytes(), [9; 16], BOB, &envelope, false).is_err());
        assert!(decrypt_dm(&bob, &alice.public_bytes(), ALICE, [9; 16], &envelope, false).is_err());
        let other = encrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, "r2", b"hola").unwrap();
        assert_eq!(
            decrypt_dm(&bob, &alice.public_bytes(), ALICE, BOB, &other, false)
                .unwrap()
                .request_id,
            "r2"
        );
    }

    #[test]
    fn tampering_is_rejected() {
        let (alice, bob) = pair();
        let envelope = encrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, "r", b"hola").unwrap();
        let raw = URL_SAFE_NO_PAD.decode(&envelope[ENVELOPE_PREFIX.len()..]).unwrap();
        for position in [10, raw.len() / 2, raw.len() - 1] {
            let mut bad = raw.clone();
            bad[position] ^= 1;
            let forged = format!("{ENVELOPE_PREFIX}{}", URL_SAFE_NO_PAD.encode(bad));
            // The recipient's box or the sender's: whichever was touched.
            let as_recipient = decrypt_dm(&bob, &alice.public_bytes(), ALICE, BOB, &forged, false);
            let as_sender = decrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, &forged, true);
            assert!(as_recipient.is_err() || as_sender.is_err());
        }
        for junk in ["", "e2e1:", "e2e1:!!", "plain text", "e2e1:AAAA"] {
            assert!(decrypt_dm(&bob, &alice.public_bytes(), ALICE, BOB, junk, false).is_err());
        }
    }

    #[test]
    fn every_message_gets_its_own_key() {
        let (alice, bob) = pair();
        let a = encrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, "r", b"igual").unwrap();
        let b = encrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, "r", b"igual").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn messages_are_independent_of_each_other() {
        // Lost and reordered messages never block the rest.
        let (alice, bob) = pair();
        let envelopes: Vec<String> = (0..5)
            .map(|i| {
                encrypt_dm(
                    &alice,
                    &bob.public_bytes(),
                    ALICE,
                    BOB,
                    &format!("r{i}"),
                    format!("m{i}").as_bytes(),
                )
                .unwrap()
            })
            .collect();
        for i in [3, 0, 4] {
            let plain = decrypt_dm(&bob, &alice.public_bytes(), ALICE, BOB, &envelopes[i], false).unwrap();
            assert_eq!(plain.plaintext, format!("m{i}").as_bytes());
        }
    }

    #[test]
    fn size_limits_and_low_order_keys_are_refused() {
        let (alice, bob) = pair();
        let big = vec![b'x'; MAX_PLAINTEXT + 1];
        assert_eq!(
            encrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, "r", &big),
            Err(E2eError::TooLarge)
        );
        let max = vec![b'x'; MAX_PLAINTEXT];
        assert!(encrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, "r", &max).is_ok());
        assert_eq!(
            encrypt_dm(&alice, &[0; 32], ALICE, BOB, "r", b"hola"),
            Err(E2eError::BadKey)
        );
    }

    #[test]
    fn envelopes_are_recognised_by_shape() {
        let (alice, bob) = pair();
        let envelope = encrypt_dm(&alice, &bob.public_bytes(), ALICE, BOB, "r", b"hola").unwrap();
        assert!(is_envelope(&envelope));
        for not in ["", "e2e1:", "hola", "e2e1:con espacios", "e2e1:a+b/c=", "E2E1:abc"] {
            assert!(!is_envelope(not), "{not:?}");
        }
    }

    #[test]
    fn public_keys_round_trip_as_text() {
        let key = Identity::generate().public_bytes();
        assert_eq!(decode_public_key(&encode_public_key(&key)), Some(key));
        for bad in ["", "AAAA", "!!!", &URL_SAFE_NO_PAD.encode([1_u8; 31])] {
            assert_eq!(decode_public_key(bad), None);
        }
    }

    #[test]
    fn identities_survive_a_round_trip_through_their_secret() {
        let alice = Identity::generate();
        let again = Identity::from_secret(alice.secret_bytes());
        assert_eq!(alice.public_bytes(), again.public_bytes());
    }

    #[test]
    fn safety_numbers_match_on_both_sides_and_change_with_a_key() {
        let (a, b) = pair();
        let (pa, pb) = (a.public_bytes(), b.public_bytes());
        assert_eq!(safety_number(&pa, &pb), safety_number(&pb, &pa));
        assert_eq!(safety_number(&pa, &pb).split(' ').count(), 6);
        assert_ne!(
            safety_number(&pa, &pb),
            safety_number(&pa, &Identity::generate().public_bytes())
        );
    }
}
