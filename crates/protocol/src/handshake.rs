use crate::error::ProtocolError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// First message on every hub WebSocket connection, sent as **plaintext**
/// JSON (no session key exists yet). Carries the client's persistent
/// identity plus a fresh ephemeral X25519 public key for this connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeHello {
    pub client_id: Uuid,
    pub client_ephemeral_pubkey: [u8; 32],
}

/// The server's reply, also plaintext. From this point on, both sides have
/// everything needed to derive the session's AEAD keys (see
/// `lan-bridge-crypto`), so every subsequent frame — starting with the
/// client's `AuthRequest` — is encrypted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeWelcome {
    pub server_id: Uuid,
    pub server_name: String,
    pub requires_password: bool,
    pub server_ephemeral_pubkey: [u8; 32],
}

/// The client's first *encrypted* message, choosing how it wants to
/// authenticate based on what it already knows locally and what
/// `HandshakeWelcome.requires_password` said.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AuthRequest {
    /// No stored PSK for this server yet, and the server doesn't require a
    /// password — request a fresh one.
    Fresh,
    /// No stored PSK yet, and the server does require a password.
    Password { password: String },
    /// A stored PSK exists; proves possession of it without resending it.
    Proof { hmac: [u8; 32] },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AuthResponse {
    /// `psk` is `Some` only when a new key was just issued (`Fresh` /
    /// `Password` auth); a successful `Proof` reauthentication returns
    /// `None` since the client already has the key.
    Accepted { psk: Option<[u8; 32]> },
    /// `retry_as_new` tells the client it's safe to immediately retry the
    /// whole handshake as if connecting for the first time (clearing its
    /// stored PSK) without bothering the user — the case where the server
    /// purged this client's record after 30 days of inactivity.
    Rejected { message: String, retry_as_new: bool },
}

impl HandshakeHello {
    pub fn to_json(&self) -> Result<String, ProtocolError> {
        serde_json::to_string(self).map_err(ProtocolError::JsonSerialize)
    }

    pub fn from_json(text: &str) -> Result<Self, ProtocolError> {
        serde_json::from_str(text).map_err(ProtocolError::JsonDeserialize)
    }
}

impl HandshakeWelcome {
    pub fn to_json(&self) -> Result<String, ProtocolError> {
        serde_json::to_string(self).map_err(ProtocolError::JsonSerialize)
    }

    pub fn from_json(text: &str) -> Result<Self, ProtocolError> {
        serde_json::from_str(text).map_err(ProtocolError::JsonDeserialize)
    }
}

impl AuthRequest {
    pub fn to_json(&self) -> Result<Vec<u8>, ProtocolError> {
        serde_json::to_vec(self).map_err(ProtocolError::JsonSerialize)
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, ProtocolError> {
        serde_json::from_slice(bytes).map_err(ProtocolError::JsonDeserialize)
    }
}

impl AuthResponse {
    pub fn to_json(&self) -> Result<Vec<u8>, ProtocolError> {
        serde_json::to_vec(self).map_err(ProtocolError::JsonSerialize)
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, ProtocolError> {
        serde_json::from_slice(bytes).map_err(ProtocolError::JsonDeserialize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_hello_roundtrips() {
        let hello = HandshakeHello {
            client_id: Uuid::new_v4(),
            client_ephemeral_pubkey: [9_u8; 32],
        };
        let json = hello.to_json().unwrap();
        let decoded = HandshakeHello::from_json(&json).unwrap();
        assert_eq!(decoded.client_id, hello.client_id);
        assert_eq!(decoded.client_ephemeral_pubkey, hello.client_ephemeral_pubkey);
    }

    #[test]
    fn auth_request_variants_roundtrip() {
        for request in [
            AuthRequest::Fresh,
            AuthRequest::Password {
                password: "hunter2".to_string(),
            },
            AuthRequest::Proof { hmac: [1_u8; 32] },
        ] {
            let bytes = request.to_json().unwrap();
            let decoded = AuthRequest::from_json(&bytes).unwrap();
            assert_eq!(format!("{request:?}"), format!("{decoded:?}"));
        }
    }
}
