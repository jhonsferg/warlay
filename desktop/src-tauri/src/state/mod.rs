use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

use crate::agent::state::DaemonState;
use crate::network::RelayClientHandle;
use crate::storage::ClientStore;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use tauri::AppHandle;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Managed Tauri state. Every slot is a singleton by design — starting a new
/// connection/tunnel/poller/process replaces (and stops) whatever was
/// running before it. `store` is the one field that's never replaced: it's
/// the local database, opened once at startup (see `lib.rs`).
pub struct AppState {
    pub store: ClientStore,
    /// LAN-mesh mode: this machine's own broadcast-reflection peer.
    pub relay_client: Mutex<Option<RelayClientHandle>>,
    /// LAN-mesh mode: background poller for a selected relay's game list.
    pub game_poller: Mutex<Option<JoinHandle<()>>>,
    /// Whichever server currently owns the one real game session this
    /// machine can have active at a time. Set when this client creates or
    /// joins a room, cleared when that room is left, deleted, or removed out
    /// from under it. Lets code that isn't itself scoped to a specific
    /// server (the LAN-mesh relay callbacks) find the one server it should
    /// actually talk to.
    pub active_room_server: Mutex<Option<Uuid>>,
    /// Watches the launched game process for exit so the UI can show "saliste
    /// de la partida" and auto-leave the room.
    pub game_exit_watcher: Mutex<Option<JoinHandle<()>>>,
    /// The agent now runs inside this same process (see `src/agent/`): its
    /// mutable state lives here, and commands talk to it directly instead of
    /// over a named pipe. `None` until `.setup()` finishes booting it.
    pub agent_state: Mutex<Option<Arc<TokioMutex<DaemonState>>>>,
    /// Cloned `AppHandle` so the agent's event pushes (hub status, tunnel
    /// status, voice frames, ...) can be emitted as Tauri events to the
    /// frontend instead of being written to a pipe. Set in `.setup()`.
    pub app_handle: Mutex<Option<AppHandle>>,
    /// The one shared source of truth for "is the configured game exe
    /// currently running", written by three independent code paths
    /// (`launch_game`'s optimistic emit, its exit-watcher, and the
    /// background poller in `lib.rs`'s `.setup()`) so they can never
    /// disagree with each other -- a poller with its own private
    /// last-known-state local would otherwise miss a launch/exit pair that
    /// both happen between two of its ticks (e.g. a process killed almost
    /// immediately after launch), permanently believing "still running"
    /// forever after.
    pub game_running: AtomicBool,
    /// Job Object tying the currently-launched game process (and its whole
    /// descendant tree, however many hops of launcher-stub indirection it
    /// goes through) to this process's own lifetime -- see
    /// `launcher::create_kill_on_close_job`'s doc comment. `None` until the
    /// first `launch_game`, and also whenever any game currently running was
    /// started outside Warlay's own launch button -- there's nothing to hold
    /// a job handle to in that case, so the older filename-match fallback
    /// (`launcher::is_process_running_at`) is used instead.
    pub game_job: Mutex<Option<crate::launcher::GameJob>>,
    /// The running desktop-audio share (capture + encoder), if any.
    pub desktop_audio: Mutex<Option<crate::desktop_audio::DesktopAudioShare>>,
    /// The global mute shortcut currently registered (only while in voice).
    pub voice_shortcut: Mutex<Option<tauri_plugin_global_shortcut::Shortcut>>,
}

impl AppState {
    pub fn new(store: ClientStore) -> Self {
        Self {
            store,
            relay_client: Mutex::new(None),
            game_poller: Mutex::new(None),
            active_room_server: Mutex::new(None),
            game_exit_watcher: Mutex::new(None),
            agent_state: Mutex::new(None),
            app_handle: Mutex::new(None),
            game_running: AtomicBool::new(false),
            game_job: Mutex::new(None),
            desktop_audio: Mutex::new(None),
            voice_shortcut: Mutex::new(None),
        }
    }
}

#[cfg(test)]
pub mod test_support;
