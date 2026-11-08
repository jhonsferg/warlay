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
        let store = ClientStore::open(&dir.join("nested").join("warlay.db")).expect("open store");
        Self { dir, store }
    }
    fn db_path(&self) -> std::path::PathBuf {
        self.dir.join("nested").join("warlay.db")
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
fn servers_can_be_added_listed_and_removed() {
    let t = TempStore::new();
    let added = t.store.add_server("hub.example:9000").unwrap();
    assert_eq!(added.address, "hub.example:9000");
    assert!(!added.has_credentials);

    let listed = t.store.list_servers().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, added.id);

    t.store.remove_server(Uuid::parse_str(&added.id).unwrap()).unwrap();
    assert!(t.store.list_servers().unwrap().is_empty());
}

#[test]
fn handshake_success_stores_the_psk_and_clearing_it_forgets_it() {
    let t = TempStore::new();
    let added = t.store.add_server("hub:1").unwrap();
    let local_id = Uuid::parse_str(&added.id).unwrap();
    let remote_id = Uuid::new_v4();
    let psk = [7u8; 32];

    t.store
        .record_handshake_success(local_id, remote_id, "Hub", Some(&psk))
        .unwrap();
    let record = t.store.get_server(local_id).unwrap().expect("server exists");
    assert_eq!(record.psk, Some(psk));
    assert_eq!(record.server_id, Some(remote_id));
    let dto = &t.store.list_servers().unwrap()[0];
    assert!(dto.has_credentials);
    assert_eq!(dto.name, "Hub");
    assert!(dto.last_connected_at.is_some());

    t.store.clear_server_psk(local_id).unwrap();
    assert_eq!(t.store.get_server(local_id).unwrap().unwrap().psk, None);
}

#[test]
fn a_reconnect_without_a_new_psk_keeps_the_stored_one() {
    let t = TempStore::new();
    let local_id = Uuid::parse_str(&t.store.add_server("hub:1").unwrap().id).unwrap();
    let psk = [9u8; 32];
    t.store
        .record_handshake_success(local_id, Uuid::new_v4(), "Hub", Some(&psk))
        .unwrap();
    t.store
        .record_handshake_success(local_id, Uuid::new_v4(), "Hub renamed", None)
        .unwrap();
    assert_eq!(t.store.get_server(local_id).unwrap().unwrap().psk, Some(psk));
}

#[test]
fn an_unknown_server_is_none_and_a_wrong_sized_psk_is_ignored() {
    let t = TempStore::new();
    assert!(t.store.get_server(Uuid::new_v4()).unwrap().is_none());

    let local_id = Uuid::parse_str(&t.store.add_server("hub:1").unwrap().id).unwrap();
    t.store
        .conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE servers SET psk = ?1 WHERE id = ?2",
            params![vec![1u8, 2, 3], local_id.to_string()],
        )
        .unwrap();
    assert_eq!(t.store.get_server(local_id).unwrap().unwrap().psk, None);
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
fn corrupt_stored_settings_fall_back_to_defaults() {
    let t = TempStore::new();
    t.store
        .conn
        .lock()
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('app_settings', 'not json')",
            [],
        )
        .unwrap();
    let settings = t.store.get_settings().unwrap();
    assert_eq!(settings.console_log_level, "info");
}
