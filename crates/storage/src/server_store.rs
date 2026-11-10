use crate::error::StorageError;
use crate::file::{ensure_parent, load_json, retire_legacy, save_json};
use crate::legacy::{self, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ClientRecord {
    pub client_id: Uuid,
    pub psk: [u8; 32],
    pub created_at: i64,
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredClient {
    /// Hex.
    psk: String,
    created_at: i64,
    last_seen_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Data {
    server_id: String,
    clients: BTreeMap<String, StoredClient>,
}

/// Persists everything the hub needs to survive a restart: its own
/// identity, and the per-client pre-shared keys issued after each client's
/// first successful authentication. One JSON file, rewritten atomically on
/// every change; synchronous by design (these calls only happen around
/// connection setup, not on the data path), so callers on an async runtime
/// can wrap them in `spawn_blocking` if they want to avoid blocking the
/// executor thread.
pub struct ServerStore {
    path: PathBuf,
    data: Mutex<Data>,
    server_id: Uuid,
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

impl ServerStore {
    /// Opens the store at `path`. If there is no file yet and a SQLite
    /// database named `warlay.db` sits next to it (what earlier versions
    /// wrote), its identity and clients are imported once and that file is
    /// renamed to `warlay.db.migrated`.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        ensure_parent(path)?;
        let existing: Option<Data> = load_json(path)?;
        let mut data = match existing {
            Some(data) => data,
            None => {
                let legacy = path.with_file_name("warlay.db");
                let imported = Self::import_legacy(&legacy);
                if imported.is_some() {
                    retire_legacy(&legacy);
                }
                imported.unwrap_or_else(|| Data {
                    server_id: Uuid::new_v4().to_string(),
                    clients: BTreeMap::new(),
                })
            }
        };
        let server_id = match Uuid::parse_str(&data.server_id) {
            Ok(id) => id,
            Err(_) => {
                let fresh = Uuid::new_v4();
                data.server_id = fresh.to_string();
                fresh
            }
        };
        save_json(path, &data)?;
        Ok(Self {
            path: path.to_path_buf(),
            data: Mutex::new(data),
            server_id,
        })
    }

    /// The identity and clients of an old SQLite database, or `None` if there
    /// is none or it cannot be read.
    fn import_legacy(legacy: &Path) -> Option<Data> {
        let identity = legacy::read_table(legacy, "server_identity").ok().flatten()?;
        let server_id = identity.iter().find_map(|row| match row.get(1) {
            Some(Value::Text(id)) if Uuid::parse_str(id).is_ok() => Some(id.clone()),
            _ => None,
        })?;
        let mut clients = BTreeMap::new();
        for row in legacy::read_table(legacy, "clients").ok().flatten().unwrap_or_default() {
            if let [Value::Text(id), Value::Blob(psk), Value::Int(created_at), Value::Int(last_seen_at)] =
                row.as_slice()
            {
                if psk.len() == 32 && Uuid::parse_str(id).is_ok() {
                    clients.insert(
                        id.clone(),
                        StoredClient {
                            psk: to_hex(psk),
                            created_at: *created_at,
                            last_seen_at: *last_seen_at,
                        },
                    );
                }
            }
        }
        Some(Data { server_id, clients })
    }

    pub fn server_id(&self) -> Uuid {
        self.server_id
    }

    pub fn get_client(&self, client_id: Uuid) -> Result<Option<ClientRecord>, StorageError> {
        let data = self.data.lock().expect("storage lock poisoned");
        let Some(stored) = data.clients.get(&client_id.to_string()) else {
            return Ok(None);
        };
        let bytes = from_hex(&stored.psk).unwrap_or_default();
        let psk: [u8; 32] = bytes
            .clone()
            .try_into()
            .map_err(|_| StorageError::MalformedPsk(bytes.len()))?;
        Ok(Some(ClientRecord {
            client_id,
            psk,
            created_at: stored.created_at,
            last_seen_at: stored.last_seen_at,
        }))
    }

    /// Registers a brand-new client with a freshly issued PSK, or overwrites
    /// an existing record (e.g. re-registration after the old one aged out).
    pub fn upsert_new_client(&self, client_id: Uuid, psk: &[u8; 32]) -> Result<(), StorageError> {
        let now = unix_now();
        self.change(|data| {
            data.clients.insert(
                client_id.to_string(),
                StoredClient {
                    psk: to_hex(psk),
                    created_at: now,
                    last_seen_at: now,
                },
            );
        })
    }

    /// Refreshes `last_seen_at` for a client that just proved possession of
    /// its existing PSK, keeping it out of the next inactivity purge.
    pub fn touch_client(&self, client_id: Uuid) -> Result<(), StorageError> {
        let now = unix_now();
        self.change(|data| {
            if let Some(client) = data.clients.get_mut(&client_id.to_string()) {
                client.last_seen_at = now;
            }
        })
    }

    /// Deletes every client whose `last_seen_at` is older than `max_age_secs`
    /// ago, returning how many were removed. A client purged this way looks
    /// brand new the next time it connects: abandoned devices do not
    /// accumulate credentials on the server forever.
    pub fn purge_inactive(&self, max_age_secs: i64) -> Result<usize, StorageError> {
        let cutoff = unix_now() - max_age_secs;
        let mut removed = 0;
        self.change(|data| {
            let before = data.clients.len();
            data.clients.retain(|_, client| client.last_seen_at >= cutoff);
            removed = before - data.clients.len();
        })?;
        Ok(removed)
    }

    /// Applies `edit` and writes the file; if the write fails, memory goes
    /// back to what the file holds.
    fn change(&self, edit: impl FnOnce(&mut Data)) -> Result<(), StorageError> {
        let mut data = self.data.lock().expect("storage lock poisoned");
        let before = data.clone();
        edit(&mut data);
        if let Err(e) = save_json(&self.path, &*data) {
            *data = before;
            return Err(e);
        }
        Ok(())
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh temp directory, removed on drop.
    struct Dir(PathBuf);

    impl Dir {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("warlay-storage-test-{}", Uuid::new_v4())))
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn opening_twice_reuses_the_same_server_id() {
        let dir = Dir::new();
        let first_id = ServerStore::open(&dir.path("hub.json")).unwrap().server_id();
        let second_id = ServerStore::open(&dir.path("hub.json")).unwrap().server_id();
        assert_eq!(first_id, second_id);
    }

    #[test]
    fn upsert_then_get_round_trips_and_survives_reopening() {
        let dir = Dir::new();
        let store = ServerStore::open(&dir.path("hub.json")).unwrap();
        let client_id = Uuid::new_v4();
        let psk = [7_u8; 32];

        store.upsert_new_client(client_id, &psk).unwrap();
        let record = store.get_client(client_id).unwrap().expect("client should exist");
        assert_eq!(record.psk, psk);
        assert_eq!(record.created_at, record.last_seen_at);

        let reopened = ServerStore::open(&dir.path("hub.json")).unwrap();
        assert_eq!(reopened.get_client(client_id).unwrap().unwrap().psk, psk);
    }

    #[test]
    fn unknown_client_returns_none() {
        let dir = Dir::new();
        let store = ServerStore::open(&dir.path("hub.json")).unwrap();
        assert!(store.get_client(Uuid::new_v4()).unwrap().is_none());
    }

    #[test]
    fn touch_client_updates_last_seen() {
        let dir = Dir::new();
        let store = ServerStore::open(&dir.path("hub.json")).unwrap();
        let client_id = Uuid::new_v4();
        store.upsert_new_client(client_id, &[1_u8; 32]).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(1100));
        store.touch_client(client_id).unwrap();

        let record = store.get_client(client_id).unwrap().unwrap();
        assert!(record.last_seen_at > record.created_at);
    }

    #[test]
    fn purge_inactive_removes_only_stale_clients() {
        let dir = Dir::new();
        let store = ServerStore::open(&dir.path("hub.json")).unwrap();
        let fresh_client = Uuid::new_v4();
        let stale_client = Uuid::new_v4();
        store.upsert_new_client(fresh_client, &[1_u8; 32]).unwrap();
        store.upsert_new_client(stale_client, &[2_u8; 32]).unwrap();

        // Make the "stale" client old directly, since this test should not
        // sleep for 30 real days.
        store
            .change(|data| data.clients.get_mut(&stale_client.to_string()).unwrap().last_seen_at = 0)
            .unwrap();

        assert_eq!(store.purge_inactive(30 * 24 * 60 * 60).unwrap(), 1);
        assert!(store.get_client(fresh_client).unwrap().is_some());
        assert!(store.get_client(stale_client).unwrap().is_none());
    }

    #[test]
    fn a_damaged_file_is_set_aside_instead_of_stopping_the_hub() {
        let dir = Dir::new();
        std::fs::create_dir_all(&dir.0).unwrap();
        std::fs::write(dir.path("hub.json"), b"{ not json").unwrap();
        let store = ServerStore::open(&dir.path("hub.json")).unwrap();
        assert!(store.get_client(Uuid::new_v4()).unwrap().is_none());
        assert!(dir.path("hub.json.corrupt").exists());
    }

    #[test]
    fn the_sqlite_database_of_earlier_versions_is_imported_once() {
        let dir = Dir::new();
        std::fs::create_dir_all(&dir.0).unwrap();
        // A real database written by SQLite with the old schema: one identity
        // and two clients (see tests/fixtures/README.md).
        std::fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/hub.sqlite3"),
            dir.path("warlay.db"),
        )
        .unwrap();
        let store = ServerStore::open(&dir.path("hub.json")).unwrap();
        assert_eq!(store.server_id().to_string(), "11111111-2222-3333-4444-555555555555");
        let first = store
            .get_client(Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000001").unwrap())
            .unwrap()
            .expect("first client imported");
        assert_eq!(first.psk, [0xAB; 32]);
        assert_eq!((first.created_at, first.last_seen_at), (1_700_000_000, 1_700_000_500));
        assert!(store
            .get_client(Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000002").unwrap())
            .unwrap()
            .is_some());
        // The old file is kept under another name, and a second open does not import again.
        assert!(!dir.path("warlay.db").exists());
        assert!(dir.path("warlay.db.migrated").exists());
        assert_eq!(
            ServerStore::open(&dir.path("hub.json")).unwrap().server_id(),
            store.server_id()
        );
    }
}
