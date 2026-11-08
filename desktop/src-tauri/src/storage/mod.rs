use crate::core::dto::AppSettings;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone)]
pub struct ClientStore {
    conn: Arc<Mutex<Connection>>,
}

impl ClientStore {
    pub fn open(path: &std::path::Path) -> Result<Self, rusqlite::Error> {
        // On a genuinely fresh install, %APPDATA%\com.warlay.lanlauncher\
        // doesn't exist yet -- SQLite can't create the db file inside a
        // missing directory, `Connection::open` fails, and since `run()`
        // calls this via `.expect(...)` before any logging or window setup
        // exists, the whole app panics (release builds abort) at the very
        // first line, with nothing visible to explain why.
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT
            )",
            [],
        )?;

        let client_id_query: Result<String, _> =
            conn.query_row("SELECT value FROM settings WHERE key = 'client_id'", [], |row| {
                row.get(0)
            });
        if client_id_query.is_err() {
            let new_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('client_id', ?1)",
                params![new_id],
            )?;
        }

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn client_id(&self) -> Uuid {
        let conn = self.conn.lock().unwrap();
        let id_str: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'client_id'", [], |row| {
                row.get(0)
            })
            .unwrap_or_else(|_| Uuid::new_v4().to_string());
        Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4())
    }

    /// Remembers when this version of the app was first run, which is when it
    /// was installed or last updated: the stored moment is kept while the
    /// version is the same and restarted when it changes. Returns that moment
    /// (Unix seconds).
    pub fn note_running_version(&self, version: &str, now_unix: i64) -> Result<i64, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let read = |key: &str| -> Option<String> {
            conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| {
                row.get(0)
            })
            .ok()
        };
        if read("running_version").as_deref() == Some(version) {
            if let Some(at) = read("running_version_since").and_then(|v| v.parse().ok()) {
                return Ok(at);
            }
        }
        for (key, value) in [
            ("running_version", version.to_string()),
            ("running_version_since", now_unix.to_string()),
        ] {
            conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                params![key, value],
            )?;
        }
        Ok(now_unix)
    }

    pub fn get_settings(&self) -> Result<AppSettings, rusqlite::Error> {
        let json: Result<String, _> = {
            let conn = self.conn.lock().unwrap();
            conn.query_row("SELECT value FROM settings WHERE key = 'app_settings'", [], |row| {
                row.get(0)
            })
        };

        if let Ok(j) = json {
            if let Ok(s) = serde_json::from_str(&j) {
                return Ok(s);
            }
        }

        Ok(AppSettings {
            display_name: format!(
                "Player_{}",
                self.client_id().to_string().chars().take(4).collect::<String>()
            ),
            game_executable_path: "C:\\Program Files (x86)\\Warcraft III\\Warcraft III.exe".to_string(),
            game_args: "-window -nativefullscr".to_string(),
            game_version: 126,
            slots_total: 10,
            console_log_level: "info".to_string(),
            file_log_level: "debug".to_string(),
            chat_message_limit: 100,
            master_volume: 1.0,
            muted: false,
            graphics_api: String::new(),
            audio_output_device_id: String::new(),
            log_retention_days: 7,
            mic_consent: false,
            mic_device_id: String::new(),
            noise_suppression: "standard".to_string(),
            share_desktop_audio: false,
        })
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let json = serde_json::to_string(settings).unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('app_settings', ?1)",
            params![json],
        )?;
        Ok(())
    }

    pub fn set_console_log_level(&self, level: &str) -> Result<(), rusqlite::Error> {
        let mut settings = self.get_settings()?;
        settings.console_log_level = level.to_string();
        self.save_settings(&settings)
    }

    pub fn set_file_log_level(&self, level: &str) -> Result<(), rusqlite::Error> {
        let mut settings = self.get_settings()?;
        settings.file_log_level = level.to_string();
        self.save_settings(&settings)
    }

    pub fn set_mic_consent(&self, granted: bool) -> Result<(), rusqlite::Error> {
        let mut settings = self.get_settings()?;
        settings.mic_consent = granted;
        self.save_settings(&settings)
    }

    pub fn set_log_retention_days(&self, days: u32) -> Result<(), rusqlite::Error> {
        let mut settings = self.get_settings()?;
        settings.log_retention_days = days;
        self.save_settings(&settings)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
