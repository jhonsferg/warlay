use super::*;

/// A store backed by a file in its own fresh temp directory (which does not
/// exist yet, like a first run), removed on drop.
struct TempStore {
    dir: std::path::PathBuf,
    store: ClientStore,
}

impl TempStore {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("warlay-store-test-{}", Uuid::new_v4()));
        let store = ClientStore::open(&dir.join("nested").join("settings.json")).expect("open store");
        Self { dir, store }
    }
    fn db_path(&self) -> std::path::PathBuf {
        self.dir.join("nested").join("settings.json")
    }
}

impl Drop for TempStore {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn open_creates_the_missing_directory_and_a_client_id() {
    let t = TempStore::new();
    assert!(t.db_path().exists());
    assert_ne!(t.store.client_id(), Uuid::nil());
}

#[test]
fn client_id_is_stable_across_reopens() {
    let t = TempStore::new();
    let first = t.store.client_id();
    let reopened = ClientStore::open(&t.db_path()).unwrap();
    assert_eq!(reopened.client_id(), first);
}

#[test]
fn default_settings_derive_the_name_from_the_client_id() {
    let t = TempStore::new();
    let settings = t.store.get_settings().unwrap();
    let prefix: String = t.store.client_id().to_string().chars().take(4).collect();
    assert_eq!(settings.display_name, format!("Player_{prefix}"));
    assert_eq!(settings.console_log_level, "info");
    assert_eq!(settings.file_log_level, "debug");
}

#[test]
fn settings_round_trip_and_log_level_setters_only_touch_their_field() {
    let t = TempStore::new();
    let mut settings = t.store.get_settings().unwrap();
    settings.display_name = "Arthas".into();
    settings.slots_total = 4;
    t.store.save_settings(&settings).unwrap();

    t.store.set_console_log_level("warn").unwrap();
    t.store.set_file_log_level("trace").unwrap();
    t.store.set_log_retention_days(30).unwrap();

    let loaded = t.store.get_settings().unwrap();
    assert_eq!(loaded.display_name, "Arthas");
    assert_eq!(loaded.slots_total, 4);
    assert_eq!(loaded.console_log_level, "warn");
    assert_eq!(loaded.file_log_level, "trace");
    assert_eq!(loaded.log_retention_days, 30);
}

#[test]
fn microphone_consent_starts_off_and_is_remembered_without_touching_other_settings() {
    let t = TempStore::new();
    let mut settings = t.store.get_settings().unwrap();
    assert!(!settings.mic_consent);
    settings.display_name = "Arthas".into();
    settings.noise_suppression = "strong".into();
    t.store.save_settings(&settings).unwrap();

    t.store.set_mic_consent(true).unwrap();
    let loaded = t.store.get_settings().unwrap();
    assert!(loaded.mic_consent);
    assert_eq!(loaded.display_name, "Arthas");
    assert_eq!(loaded.noise_suppression, "strong");

    // Survives a restart, and can be withdrawn.
    let reopened = ClientStore::open(&t.db_path()).unwrap();
    assert!(reopened.get_settings().unwrap().mic_consent);
    reopened.set_mic_consent(false).unwrap();
    assert!(!reopened.get_settings().unwrap().mic_consent);
}

#[test]
fn corrupt_stored_settings_fall_back_to_defaults() {
    let t = TempStore::new();
    t.store.kv.set("app_settings", "not json").unwrap();
    let settings = t.store.get_settings().unwrap();
    assert_eq!(settings.console_log_level, "info");
}

#[test]
fn the_running_version_keeps_its_first_moment_until_it_changes() {
    let t = TempStore::new();
    assert_eq!(
        t.store.note_running_version("0.10.0", 1_000).unwrap(),
        1_000,
        "first run"
    );
    assert_eq!(
        t.store.note_running_version("0.10.0", 9_999).unwrap(),
        1_000,
        "same version: the moment it was installed stays"
    );
    assert_eq!(
        t.store.note_running_version("0.11.0", 5_000).unwrap(),
        5_000,
        "an update restarts it"
    );
    assert_eq!(t.store.note_running_version("0.11.0", 6_000).unwrap(), 5_000);
}

#[test]
fn the_running_version_survives_reopening_the_database() {
    let t = TempStore::new();
    t.store.note_running_version("0.10.0", 1_000).unwrap();
    let reopened = ClientStore::open(&t.db_path()).unwrap();
    assert_eq!(reopened.note_running_version("0.10.0", 7_000).unwrap(), 1_000);
}

#[test]
fn the_database_of_earlier_versions_is_imported_on_the_first_run() {
    let dir = std::env::temp_dir().join(format!("warlay-store-migrate-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    // A real SQLite database as the previous version wrote it.
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/storage/tests/fixtures/client.sqlite3"
        ),
        dir.join("warlay.db"),
    )
    .unwrap();

    let store = ClientStore::open(&dir.join("settings.json")).unwrap();
    assert_eq!(store.client_id().to_string(), "0f8fad5b-d9cb-469f-a165-70867728950e");
    let settings = store.get_settings().unwrap();
    assert_eq!(settings.display_name, "Arthas");
    assert_eq!(settings.slots_total, 4);
    assert!(settings.game_executable_path.ends_with("Frozen Throne.exe"));
    assert_eq!(store.note_running_version("0.10.0", 9_999).unwrap(), 1_700_000_000);
    assert!(dir.join("warlay.db.migrated").exists() && !dir.join("warlay.db").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
