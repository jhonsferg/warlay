use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use warlay::protocol::ipc::AgentSettings;

/// Persists the agent's own configuration -- separate from
/// `desktop/src-tauri/src/storage/mod.rs::ClientStore`, which is the
/// desktop client's own store. Living in a table named
/// `agent_settings` rather than reusing the client's `settings` table is
/// what keeps the two from stepping on each other.
#[derive(Clone)]
pub struct AgentStore {
    conn: Arc<Mutex<Connection>>,
}

/// See `paths::data_dir` (on Windows, the same file the desktop app uses).
fn db_path() -> std::path::PathBuf {
    super::paths::data_dir().join("warlay.db")
}

impl AgentStore {
    /// Infallible: this runs before `logging::init_logging` (the initial
    /// log level itself comes from here), so there's nowhere good to
    /// report a failure yet. Falls back to an in-memory connection (still
    /// usable, just doesn't survive a restart) rather than panicking --
    /// losing persistence for one session is fine, crashing the agent
    /// before it even opens its pipe is not.
    pub fn open() -> Self {
        let path = db_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(&path)
            .unwrap_or_else(|_| Connection::open_in_memory().expect("in-memory sqlite connection should never fail"));

        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_settings (
                key TEXT PRIMARY KEY,
                value TEXT
            )",
            [],
        );

        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    pub fn get_settings(&self) -> AgentSettings {
        let json: Result<String, _> = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT value FROM agent_settings WHERE key = 'agent_settings'",
                [],
                |row| row.get(0),
            )
        };

        json.ok()
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &AgentSettings) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let json = serde_json::to_string(settings).expect("AgentSettings always serializes");
        conn.execute(
            "INSERT OR REPLACE INTO agent_settings (key, value) VALUES ('agent_settings', ?1)",
            params![json],
        )?;
        Ok(())
    }
}
