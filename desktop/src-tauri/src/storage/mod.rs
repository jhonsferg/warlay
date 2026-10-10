use crate::core::dto::{AppSettings, ServerDto};
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone)]
pub struct ClientStore {
    conn: Arc<Mutex<Connection>>,
}

pub struct ServerRecord {
    pub psk: Option<[u8; 32]>,
    pub server_id: Option<Uuid>,
    pub address: String,
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
            "CREATE TABLE IF NOT EXISTS servers (
                id TEXT PRIMARY KEY,
                name TEXT,
                address TEXT,
                server_id TEXT,
                psk BLOB,
                last_connected_at INTEGER
            )",
            [],
        )?;

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

    pub fn list_servers(&self) -> Result<Vec<ServerDto>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, address, server_id, psk, last_connected_at FROM servers")?;

        let servers = stmt
            .query_map([], |row| {
                let server_id_str: Option<String> = row.get(3)?;
                let psk: Option<Vec<u8>> = row.get(4)?;

                Ok(ServerDto {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    address: row.get(2)?,
                    server_id: server_id_str,
                    has_credentials: psk.is_some(),
                    last_connected_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(servers)
    }

    pub fn get_server(&self, id: Uuid) -> Result<Option<ServerRecord>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT psk, server_id, address FROM servers WHERE id = ?1")?;

        let mut rows = stmt.query(params![id.to_string()])?;
        if let Some(row) = rows.next()? {
            let psk_vec: Option<Vec<u8>> = row.get(0)?;
            let server_id_str: Option<String> = row.get(1)?;
            let address: String = row.get(2)?;

            let psk = psk_vec.and_then(|v| {
                if v.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&v);
                    Some(arr)
                } else {
                    None
                }
            });

            let server_id = server_id_str.and_then(|s| Uuid::parse_str(&s).ok());

            Ok(Some(ServerRecord {
                psk,
                server_id,
                address,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn add_server(&self, address: &str) -> Result<ServerDto, rusqlite::Error> {
        let id = Uuid::new_v4().to_string();
        let name = format!("Server ({})", address);

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO servers (id, name, address) VALUES (?1, ?2, ?3)",
            params![id, name, address],
        )?;

        Ok(ServerDto {
            id,
            name,
            address: address.to_string(),
            server_id: None,
            has_credentials: false,
            last_connected_at: None,
        })
    }

    pub fn remove_server(&self, server_id: Uuid) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM servers WHERE id = ?1", params![server_id.to_string()])?;
        Ok(())
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
            voice_enabled: false,
            microphone_device_id: String::new(),
            mic_toggle_key: String::new(),
            voice_input_volume: 1.0,
            voice_output_volume: 1.0,
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

    pub fn set_log_retention_days(&self, days: u32) -> Result<(), rusqlite::Error> {
        let mut settings = self.get_settings()?;
        settings.log_retention_days = days;
        self.save_settings(&settings)
    }

    pub fn record_handshake_success(
        &self,
        local_id: Uuid,
        server_id: Uuid,
        server_name: &str,
        new_psk: Option<&[u8; 32]>,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        if let Some(psk) = new_psk {
            conn.execute(
                "UPDATE servers SET server_id = ?1, name = ?2, psk = ?3, last_connected_at = ?4 WHERE id = ?5",
                params![server_id.to_string(), server_name, psk, now, local_id.to_string()],
            )?;
        } else {
            conn.execute(
                "UPDATE servers SET server_id = ?1, name = ?2, last_connected_at = ?3 WHERE id = ?4",
                params![server_id.to_string(), server_name, now, local_id.to_string()],
            )?;
        }
        Ok(())
    }

    pub fn clear_server_psk(&self, local_id: Uuid) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE servers SET psk = NULL WHERE id = ?1",
            params![local_id.to_string()],
        )?;
        Ok(())
    }
}
