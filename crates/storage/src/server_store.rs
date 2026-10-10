use crate::error::StorageError;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
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

/// Persists everything the hub needs to survive a restart: its own
/// identity, and the per-client pre-shared keys issued after each client's
/// first successful authentication. Synchronous by design (SQLite calls are
/// microsecond-scale and only happen around connection setup, not on the
/// data path) — callers on an async runtime should wrap calls in
/// `spawn_blocking` if they want to avoid blocking the executor thread.
pub struct ServerStore {
    conn: Mutex<Connection>,
    server_id: Uuid,
}

impl ServerStore {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| StorageError::CreateDataDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS server_identity (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                server_id TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS clients (
                client_id TEXT PRIMARY KEY,
                psk BLOB NOT NULL,
                created_at INTEGER NOT NULL,
                last_seen_at INTEGER NOT NULL
            );
            ",
        )?;

        let server_id = Self::load_or_create_server_id(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            server_id,
        })
    }

    fn load_or_create_server_id(conn: &Connection) -> Result<Uuid, StorageError> {
        let existing: Option<String> = conn
            .query_row("SELECT server_id FROM server_identity WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()?;

        if let Some(text) = existing {
            return Ok(Uuid::parse_str(&text).unwrap_or_else(|_| Uuid::new_v4()));
        }

        let fresh = Uuid::new_v4();
        conn.execute(
            "INSERT INTO server_identity (id, server_id) VALUES (1, ?1)",
            params![fresh.to_string()],
        )?;
        Ok(fresh)
    }

    pub fn server_id(&self) -> Uuid {
        self.server_id
    }

    pub fn get_client(&self, client_id: Uuid) -> Result<Option<ClientRecord>, StorageError> {
        let conn = self.conn.lock().expect("storage lock poisoned");
        conn.query_row(
            "SELECT psk, created_at, last_seen_at FROM clients WHERE client_id = ?1",
            params![client_id.to_string()],
            |row| {
                let psk: Vec<u8> = row.get(0)?;
                Ok((psk, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
            },
        )
        .optional()?
        .map(|(psk, created_at, last_seen_at)| {
            let psk: [u8; 32] = psk
                .clone()
                .try_into()
                .map_err(|_| StorageError::MalformedPsk(psk.len()))?;
            Ok(ClientRecord {
                client_id,
                psk,
                created_at,
                last_seen_at,
            })
        })
        .transpose()
    }

    /// Registers a brand-new client with a freshly issued PSK, or overwrites
    /// an existing record (e.g. re-registration after the old one aged out).
    pub fn upsert_new_client(&self, client_id: Uuid, psk: &[u8; 32]) -> Result<(), StorageError> {
        let now = unix_now();
        let conn = self.conn.lock().expect("storage lock poisoned");
        conn.execute(
            "INSERT INTO clients (client_id, psk, created_at, last_seen_at) VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(client_id) DO UPDATE SET psk = excluded.psk, created_at = excluded.created_at, last_seen_at = excluded.last_seen_at",
            params![client_id.to_string(), psk.as_slice(), now],
        )?;
        Ok(())
    }

    /// Refreshes `last_seen_at` for a client that just proved possession of
    /// its existing PSK, keeping it out of the next inactivity purge.
    pub fn touch_client(&self, client_id: Uuid) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("storage lock poisoned");
        conn.execute(
            "UPDATE clients SET last_seen_at = ?2 WHERE client_id = ?1",
            params![client_id.to_string(), unix_now()],
        )?;
        Ok(())
    }

    /// Deletes every client whose `last_seen_at` is older than `max_age_secs`
    /// ago, returning how many were removed. A client purged this way looks
    /// brand new the next time it connects — the whole point being that
    /// abandoned devices don't accumulate credentials on the server forever.
    pub fn purge_inactive(&self, max_age_secs: i64) -> Result<usize, StorageError> {
        let cutoff = unix_now() - max_age_secs;
        let conn = self.conn.lock().expect("storage lock poisoned");
        let deleted = conn.execute("DELETE FROM clients WHERE last_seen_at < ?1", params![cutoff])?;
        Ok(deleted)
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
    use std::path::PathBuf;

    fn temp_db(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("warlay-storage-test-{name}.db"));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn opening_twice_reuses_the_same_server_id() {
        let path = temp_db("server-id");
        let first_id = ServerStore::open(&path).unwrap().server_id();
        let second_id = ServerStore::open(&path).unwrap().server_id();
        assert_eq!(first_id, second_id);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn upsert_then_get_round_trips() {
        let path = temp_db("upsert-get");
        let store = ServerStore::open(&path).unwrap();
        let client_id = Uuid::new_v4();
        let psk = [7_u8; 32];

        store.upsert_new_client(client_id, &psk).unwrap();
        let record = store.get_client(client_id).unwrap().expect("client should exist");

        assert_eq!(record.psk, psk);
        assert_eq!(record.created_at, record.last_seen_at);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unknown_client_returns_none() {
        let path = temp_db("unknown-client");
        let store = ServerStore::open(&path).unwrap();
        assert!(store.get_client(Uuid::new_v4()).unwrap().is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn touch_client_updates_last_seen() {
        let path = temp_db("touch-client");
        let store = ServerStore::open(&path).unwrap();
        let client_id = Uuid::new_v4();
        store.upsert_new_client(client_id, &[1_u8; 32]).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(1100));
        store.touch_client(client_id).unwrap();

        let record = store.get_client(client_id).unwrap().unwrap();
        assert!(record.last_seen_at > record.created_at);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn purge_inactive_removes_only_stale_clients() {
        let path = temp_db("purge-inactive");
        let store = ServerStore::open(&path).unwrap();
        let fresh_client = Uuid::new_v4();
        let stale_client = Uuid::new_v4();

        store.upsert_new_client(fresh_client, &[1_u8; 32]).unwrap();
        store.upsert_new_client(stale_client, &[2_u8; 32]).unwrap();

        // Force the "stale" client's last_seen_at far into the past directly,
        // since we don't want this test to sleep for 30 real days.
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE clients SET last_seen_at = 0 WHERE client_id = ?1",
                params![stale_client.to_string()],
            )
            .unwrap();
        }

        let deleted = store.purge_inactive(30 * 24 * 60 * 60).unwrap();
        assert_eq!(deleted, 1);
        assert!(store.get_client(fresh_client).unwrap().is_some());
        assert!(store.get_client(stale_client).unwrap().is_none());
        let _ = std::fs::remove_file(&path);
    }
}
