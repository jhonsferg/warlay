//! Small stores kept as one JSON file each: written atomically, so a crash or a
//! power cut leaves either the old file or the new one, never half of it.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::StorageError;
use crate::legacy::{self, Value};

/// Creates the parent directory, which is missing on a first run.
pub(crate) fn ensure_parent(path: &Path) -> Result<(), StorageError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|source| StorageError::CreateDataDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

/// Writes `value` to `path` through a temporary file in the same directory.
pub(crate) fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<(), StorageError> {
    let io = |source| StorageError::Io {
        path: path.to_path_buf(),
        source,
    };
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| io(std::io::Error::other(e)))?;
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".tmp-{}", std::process::id()));
    let temporary = PathBuf::from(temporary);
    let mut file = std::fs::File::create(&temporary).map_err(io)?;
    file.write_all(&bytes).map_err(io)?;
    file.sync_all().map_err(io)?;
    drop(file);
    // Replaces the old file in one step (also on Windows).
    std::fs::rename(&temporary, path).map_err(|source| {
        let _ = std::fs::remove_file(&temporary);
        StorageError::Io {
            path: path.to_path_buf(),
            source,
        }
    })
}

/// Reads `path` as JSON. A file that does not parse is moved aside as
/// `<name>.corrupt` and treated as absent, so a damaged file cannot keep the
/// program from starting; `None` also covers a missing file.
pub(crate) fn load_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, StorageError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(StorageError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(value) => Ok(Some(value)),
        Err(_) => {
            let mut aside = path.as_os_str().to_owned();
            aside.push(".corrupt");
            let _ = std::fs::rename(path, PathBuf::from(aside));
            Ok(None)
        }
    }
}

/// Where a database from the SQLite era goes once its contents are imported:
/// kept next to the new file, so nothing is lost.
pub(crate) fn retire_legacy(legacy: &Path) {
    let mut kept = legacy.as_os_str().to_owned();
    kept.push(".migrated");
    let _ = std::fs::rename(legacy, PathBuf::from(kept));
}

/// A text-to-text store in one JSON file (settings, identifiers). Reads are
/// from memory; every change is written out before it returns.
pub struct KvFile {
    path: PathBuf,
    map: Mutex<BTreeMap<String, String>>,
}

impl KvFile {
    /// Opens the store, creating its directory. If there is no file yet but a
    /// SQLite database from an earlier version is at `legacy` with a table
    /// `settings(key, value)`, its rows are imported (once) and that file is
    /// renamed to `<name>.migrated`.
    pub fn open(path: &Path, legacy: Option<&Path>) -> Result<Self, StorageError> {
        ensure_parent(path)?;
        let existing: Option<BTreeMap<String, String>> = load_json(path)?;
        let fresh = existing.is_none();
        let store = Self {
            path: path.to_path_buf(),
            map: Mutex::new(existing.unwrap_or_default()),
        };
        if let (true, Some(legacy)) = (fresh, legacy) {
            // An unreadable old database is left where it is, and the program
            // starts with an empty store rather than not at all.
            if let Ok(Some(rows)) = legacy::read_table(legacy, "settings") {
                let mut map = store.map.lock().expect("kv lock poisoned");
                for row in rows {
                    if let (Some(Value::Text(key)), Some(Value::Text(value))) = (row.first(), row.get(1)) {
                        map.insert(key.clone(), value.clone());
                    }
                }
                save_json(&store.path, &*map)?;
                drop(map);
                retire_legacy(legacy);
            }
        }
        Ok(store)
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.map.lock().expect("kv lock poisoned").get(key).cloned()
    }

    pub fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.set_many(&[(key, value.to_string())])
    }

    /// Sets several keys with one write.
    pub fn set_many(&self, entries: &[(&str, String)]) -> Result<(), StorageError> {
        let mut map = self.map.lock().expect("kv lock poisoned");
        let before = map.clone();
        for (key, value) in entries {
            map.insert((*key).to_string(), value.clone());
        }
        if let Err(e) = save_json(&self.path, &*map) {
            // What the file holds is what memory holds.
            *map = before;
            return Err(e);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("warlay-kv-test-{}", uuid::Uuid::new_v4()))
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    #[test]
    fn values_are_kept_across_reopening_and_the_directory_is_created() {
        let dir = temp_dir();
        let path = dir.join("nested").join("settings.json");
        let store = KvFile::open(&path, None).unwrap();
        assert_eq!(store.get("a"), None);
        store.set("a", "1").unwrap();
        store
            .set_many(&[("b", "2".to_string()), ("a", "3".to_string())])
            .unwrap();
        let reopened = KvFile::open(&path, None).unwrap();
        assert_eq!(
            (reopened.get("a").as_deref(), reopened.get("b").as_deref()),
            (Some("3"), Some("2"))
        );
        // No temporary file is left behind.
        assert_eq!(std::fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_file_is_set_aside_and_the_store_starts_empty() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, b"{ not json").unwrap();
        let store = KvFile::open(&path, None).unwrap();
        assert_eq!(store.get("a"), None);
        assert!(dir.join("settings.json.corrupt").exists());
        store.set("a", "1").unwrap();
        assert_eq!(KvFile::open(&path, None).unwrap().get("a").as_deref(), Some("1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sqlite_settings_of_earlier_versions_are_imported_once() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let legacy = dir.join("warlay.db");
        std::fs::copy(fixture("client.sqlite3"), &legacy).unwrap();
        let path = dir.join("settings.json");
        let store = KvFile::open(&path, Some(&legacy)).unwrap();
        assert_eq!(
            store.get("client_id").as_deref(),
            Some("0f8fad5b-d9cb-469f-a165-70867728950e")
        );
        assert_eq!(store.get("running_version").as_deref(), Some("0.10.0"));
        assert!(store.get("app_settings").unwrap().contains("Arthas"));
        assert!(!legacy.exists() && dir.join("warlay.db.migrated").exists());
        // Changes after the import win; a second open does not import again.
        store.set("running_version", "0.11.0").unwrap();
        std::fs::copy(fixture("client.sqlite3"), &legacy).unwrap();
        let again = KvFile::open(&path, Some(&legacy)).unwrap();
        assert_eq!(again.get("running_version").as_deref(), Some("0.11.0"));
        assert!(legacy.exists(), "an existing store never touches the old file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreadable_old_database_is_left_alone() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let legacy = dir.join("warlay.db");
        std::fs::write(&legacy, vec![b'x'; 300]).unwrap();
        let store = KvFile::open(&dir.join("settings.json"), Some(&legacy)).unwrap();
        assert_eq!(store.get("client_id"), None);
        assert!(legacy.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
