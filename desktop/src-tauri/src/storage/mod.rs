use crate::core::dto::AppSettings;
use std::sync::Arc;
use uuid::Uuid;
use warlay::storage::{KvFile, StorageError};

/// The app's local settings and identifiers: one small JSON file, written
/// atomically (see `warlay::storage`). Earlier versions kept them in a SQLite
/// database next to it (`warlay.db`); on the first run of this version that
/// file's contents are imported and it is renamed to `warlay.db.migrated`.
#[derive(Clone)]
pub struct ClientStore {
    kv: Arc<KvFile>,
}

impl ClientStore {
    pub fn open(path: &std::path::Path) -> Result<Self, StorageError> {
        // On a genuinely fresh install, %APPDATA%\com.warlay.lanlauncher\
        // doesn't exist yet; `KvFile::open` creates it. This is called via
        // `.expect(...)` before any logging or window setup exists, so a
        // failure here would abort the app with nothing visible to explain why.
        let legacy = path.with_file_name("warlay.db");
        let kv = KvFile::open(path, Some(&legacy))?;
        if kv.get("client_id").is_none() {
            kv.set("client_id", &Uuid::new_v4().to_string())?;
        }
        Ok(Self { kv: Arc::new(kv) })
    }

    pub fn client_id(&self) -> Uuid {
        self.kv
            .get("client_id")
            .and_then(|id| Uuid::parse_str(&id).ok())
            .unwrap_or_else(Uuid::new_v4)
    }

    /// Remembers when this version of the app was first run, which is when it
    /// was installed or last updated: the stored moment is kept while the
    /// version is the same and restarted when it changes. Returns that moment
    /// (Unix seconds).
    pub fn note_running_version(&self, version: &str, now_unix: i64) -> Result<i64, StorageError> {
        if self.kv.get("running_version").as_deref() == Some(version) {
            if let Some(at) = self.kv.get("running_version_since").and_then(|v| v.parse().ok()) {
                return Ok(at);
            }
        }
        self.kv.set_many(&[
            ("running_version", version.to_string()),
            ("running_version_since", now_unix.to_string()),
        ])?;
        Ok(now_unix)
    }

    pub fn get_settings(&self) -> Result<AppSettings, StorageError> {
        if let Some(json) = self.kv.get("app_settings") {
            if let Ok(settings) = serde_json::from_str(&json) {
                return Ok(settings);
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
            low_latency_voice: false,
        })
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), StorageError> {
        let json = serde_json::to_string(settings).expect("settings serialize");
        self.kv.set("app_settings", &json)
    }

    pub fn set_console_log_level(&self, level: &str) -> Result<(), StorageError> {
        let mut settings = self.get_settings()?;
        settings.console_log_level = level.to_string();
        self.save_settings(&settings)
    }

    pub fn set_file_log_level(&self, level: &str) -> Result<(), StorageError> {
        let mut settings = self.get_settings()?;
        settings.file_log_level = level.to_string();
        self.save_settings(&settings)
    }

    pub fn set_mic_consent(&self, granted: bool) -> Result<(), StorageError> {
        let mut settings = self.get_settings()?;
        settings.mic_consent = granted;
        self.save_settings(&settings)
    }

    pub fn set_log_retention_days(&self, days: u32) -> Result<(), StorageError> {
        let mut settings = self.get_settings()?;
        settings.log_retention_days = days;
        self.save_settings(&settings)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
