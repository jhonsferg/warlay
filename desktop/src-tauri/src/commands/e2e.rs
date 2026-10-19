//! End-to-end encryption of direct messages (see `warlay::crypto::e2e`).
//!
//! The private key lives here, in Rust: the webview asks for a message to
//! be encrypted or decrypted and only ever sees public keys and plain text.
//! The key is generated on first use, per account, and stored in the user's
//! AppData protected by the operating system (DPAPI on Windows: only that
//! Windows user can unprotect it).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use uuid::Uuid;
use warlay::crypto::e2e::{self, Identity};

use crate::core::AppError;

fn invalid(error: impl std::fmt::Display) -> AppError {
    AppError::InvalidInput(error.to_string())
}

fn identity_dir() -> PathBuf {
    let appdata = std::env::var("APPDATA")
        .or_else(|_| std::env::var("XDG_DATA_HOME"))
        .unwrap_or_else(|_| ".".to_string());
    Path::new(&appdata).join("com.warlay.lanlauncher").join("e2e")
}

/// Extra bytes mixed into the protection, so another program running as the
/// same user can't simply ask DPAPI to open the file.
#[cfg(windows)]
const ENTROPY: &[u8] = b"warlay-e2e-identity-v1";

#[cfg(windows)]
fn protect(plain: &[u8]) -> Result<Vec<u8>, AppError> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};

    let input = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `input` and `entropy` point at live slices for the call; on
    // success `output` is a LocalAlloc'd buffer that is copied and freed.
    unsafe {
        CryptProtectData(&input, PCWSTR::null(), Some(&entropy), None, None, 0, &mut output).map_err(invalid)?;
        let protected = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(protected)
    }
}

#[cfg(windows)]
fn unprotect(protected: &[u8]) -> Result<Vec<u8>, AppError> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

    let input = CRYPT_INTEGER_BLOB {
        cbData: protected.len() as u32,
        pbData: protected.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: as in `protect`.
    unsafe {
        CryptUnprotectData(&input, None, Some(&entropy), None, None, 0, &mut output).map_err(invalid)?;
        let plain = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(plain)
    }
}

// Elsewhere the file is only readable by its owner (see `write_private`).
#[cfg(not(windows))]
fn protect(plain: &[u8]) -> Result<Vec<u8>, AppError> {
    Ok(plain.to_vec())
}

#[cfg(not(windows))]
fn unprotect(protected: &[u8]) -> Result<Vec<u8>, AppError> {
    Ok(protected.to_vec())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Write beside it and rename, so a crash never leaves half a key.
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&temp, path)?;
    Ok(())
}

/// Loads the identity stored in `dir` for `user`, creating it the first time.
fn load_or_create(dir: &Path, user: Uuid) -> Result<Identity, AppError> {
    let path = dir.join(format!("{user}.key"));
    match std::fs::read(&path) {
        Ok(protected) => {
            let secret: [u8; 32] = unprotect(&protected)?
                .try_into()
                .map_err(|_| invalid("the stored key is corrupt"))?;
            Ok(Identity::from_secret(secret))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let identity = Identity::generate();
            write_private(&path, &protect(&identity.secret_bytes())?)?;
            Ok(identity)
        }
        Err(error) => Err(error.into()),
    }
}

/// Identities already opened this run, by account.
fn identity_for(user: Uuid) -> Result<Arc<Identity>, AppError> {
    static CACHE: OnceLock<Mutex<HashMap<Uuid, Arc<Identity>>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .expect("identity cache poisoned");
    if let Some(identity) = cache.get(&user) {
        return Ok(identity.clone());
    }
    let identity = Arc::new(load_or_create(&identity_dir(), user)?);
    cache.insert(user, identity.clone());
    Ok(identity)
}

fn user_id(text: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(text).map_err(|_| invalid("invalid user id"))
}

fn peer_key(text: &str) -> Result<[u8; 32], AppError> {
    e2e::decode_public_key(text).ok_or_else(|| invalid("invalid public key"))
}

/// This account's public key (base64url), creating the identity if needed.
#[tauri::command]
pub fn e2e_identity(user_id_text: String) -> Result<String, AppError> {
    Ok(e2e::encode_public_key(
        &identity_for(user_id(&user_id_text)?)?.public_bytes(),
    ))
}

/// The message body to post for `plaintext`, sent to `peer_id`.
#[tauri::command]
pub fn e2e_encrypt(
    me: String,
    peer_id: String,
    peer_public_key: String,
    request_id: String,
    plaintext: String,
) -> Result<String, AppError> {
    let (me, peer) = (user_id(&me)?, user_id(&peer_id)?);
    let identity = identity_for(me)?;
    e2e::encrypt_dm(
        &identity,
        &peer_key(&peer_public_key)?,
        *me.as_bytes(),
        *peer.as_bytes(),
        &request_id,
        plaintext.as_bytes(),
    )
    .map_err(invalid)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecryptedMessage {
    pub plaintext: String,
    pub request_id: String,
}

/// Opens a message of the conversation between `me` and `peer_id`, sent by
/// `sender_id` (one of the two).
#[tauri::command]
pub fn e2e_decrypt(
    me: String,
    peer_id: String,
    peer_public_key: String,
    sender_id: String,
    envelope: String,
) -> Result<DecryptedMessage, AppError> {
    let (me, peer, sender) = (user_id(&me)?, user_id(&peer_id)?, user_id(&sender_id)?);
    let i_am_sender = sender == me;
    let recipient = if i_am_sender { peer } else { me };
    let identity = identity_for(me)?;
    let opened = e2e::decrypt_dm(
        &identity,
        &peer_key(&peer_public_key)?,
        *sender.as_bytes(),
        *recipient.as_bytes(),
        &envelope,
        i_am_sender,
    )
    .map_err(invalid)?;
    Ok(DecryptedMessage {
        plaintext: String::from_utf8(opened.plaintext).map_err(|_| invalid("the message is not text"))?,
        request_id: opened.request_id,
    })
}

/// The code two users compare out of band to check nobody swapped a key.
#[tauri::command]
pub fn e2e_safety_number(me: String, peer_public_key: String) -> Result<String, AppError> {
    let mine = identity_for(user_id(&me)?)?.public_bytes();
    Ok(e2e::safety_number(&mine, &peer_key(&peer_public_key)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("warlay-e2e-test-{name}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_identity_is_created_once_and_loaded_after() {
        let dir = temp_dir("persist");
        let user = Uuid::new_v4();
        let first = load_or_create(&dir, user).unwrap();
        let again = load_or_create(&dir, user).unwrap();
        assert_eq!(first.public_bytes(), again.public_bytes());
        // Another account on the same machine has its own key.
        assert_ne!(
            first.public_bytes(),
            load_or_create(&dir, Uuid::new_v4()).unwrap().public_bytes()
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_private_key_is_not_stored_in_the_clear() {
        let dir = temp_dir("sealed");
        let user = Uuid::new_v4();
        let identity = load_or_create(&dir, user).unwrap();
        let stored = std::fs::read(dir.join(format!("{user}.key"))).unwrap();
        if cfg!(windows) {
            assert!(!stored.windows(32).any(|chunk| chunk == identity.secret_bytes()));
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_corrupt_key_file_is_an_error_not_a_new_identity() {
        let dir = temp_dir("corrupt");
        let user = Uuid::new_v4();
        std::fs::write(dir.join(format!("{user}.key")), b"garbage").unwrap();
        assert!(load_or_create(&dir, user).is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn two_accounts_can_talk_through_the_commands_logic() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let dir = temp_dir("talk");
        let (ida, idb) = (load_or_create(&dir, a).unwrap(), load_or_create(&dir, b).unwrap());
        let envelope = e2e::encrypt_dm(
            &ida,
            &idb.public_bytes(),
            *a.as_bytes(),
            *b.as_bytes(),
            "r",
            "hola".as_bytes(),
        )
        .unwrap();
        let opened = e2e::decrypt_dm(
            &idb,
            &ida.public_bytes(),
            *a.as_bytes(),
            *b.as_bytes(),
            &envelope,
            false,
        )
        .unwrap();
        assert_eq!(opened.plaintext, b"hola");
        std::fs::remove_dir_all(dir).ok();
    }
}
