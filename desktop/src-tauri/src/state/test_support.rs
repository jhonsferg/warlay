//! Test scaffolding: a mock Tauri app managing a real `AppState` over a
//! throwaway SQLite file, so commands can be called exactly as the frontend
//! would (through `State<'_, AppState>`).

use std::path::PathBuf;

use tauri::test::{mock_app, MockRuntime};
use tauri::{App, Manager};
use uuid::Uuid;

use super::AppState;
use crate::storage::ClientStore;

pub struct TestApp {
    pub app: App<MockRuntime>,
    dir: PathBuf,
}

impl TestApp {
    pub fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("warlay-app-test-{}", Uuid::new_v4()));
        let store = ClientStore::open(&dir.join("warlay.db")).expect("open test store");
        let app = mock_app();
        app.manage(AppState::new(store));
        Self { app, dir }
    }

    pub fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
