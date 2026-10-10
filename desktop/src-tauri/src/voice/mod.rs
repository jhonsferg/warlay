pub mod capture;
pub mod codec;
pub mod jitter;
pub mod playback;

use crate::daemon_ipc::DaemonClient;
use capture::AudioCapture;
use playback::AudioPlayback;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tauri_plugin_global_shortcut::Shortcut;

#[derive(Clone)]
pub struct VoiceManager {
    capture: Arc<Mutex<AudioCapture>>,
    playback: Arc<Mutex<AudioPlayback>>,
    muted_peers: Arc<Mutex<HashSet<String>>>,
}

impl VoiceManager {
    pub fn new() -> Self {
        Self {
            capture: Arc::new(Mutex::new(AudioCapture::new())),
            playback: Arc::new(Mutex::new(AudioPlayback::new())),
            muted_peers: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn toggle_mute(&self, client_id: &str) -> bool {
        if let Ok(mut set) = self.muted_peers.lock() {
            if set.contains(client_id) {
                set.remove(client_id);
                false
            } else {
                set.insert(client_id.to_string());
                true
            }
        } else {
            false
        }
    }

    pub fn is_muted(&self, client_id: &str) -> bool {
        if let Ok(set) = self.muted_peers.lock() {
            set.contains(client_id)
        } else {
            false
        }
    }

    pub fn muted_peers(&self) -> Vec<String> {
        if let Ok(set) = self.muted_peers.lock() {
            let mut list: Vec<String> = set.iter().cloned().collect();
            list.sort();
            list
        } else {
            Vec::new()
        }
    }

    pub fn start_voice(
        &self,
        room_token: String,
        daemon_client: DaemonClient,
        input_device_id: Option<String>,
        output_device_id: Option<String>,
    ) -> Result<(), String> {
        let mut capture = self.capture.lock().map_err(|_| "poisoned lock")?;
        let mut playback = self.playback.lock().map_err(|_| "poisoned lock")?;

        capture.start(room_token, daemon_client, input_device_id)?;
        playback.start(output_device_id)?;

        Ok(())
    }

    pub fn stop_voice(&self) {
        if let Ok(mut capture) = self.capture.lock() {
            capture.stop();
        }
        if let Ok(mut playback) = self.playback.lock() {
            playback.stop();
        }
    }

    pub fn handle_voice_frame_in(&self, from_client_id: String, opus_bytes: Vec<u8>) {
        if self.is_muted(&from_client_id) {
            return;
        }
        if let Ok(playback) = self.playback.lock() {
            playback.push_frame(from_client_id, opus_bytes);
        }
    }

    pub fn set_self_mic_muted(&self, muted: bool) {
        if let Ok(capture) = self.capture.lock() {
            capture.set_self_muted(muted);
        }
    }

    pub fn is_self_mic_muted(&self) -> bool {
        if let Ok(capture) = self.capture.lock() {
            capture.is_self_muted()
        } else {
            false
        }
    }
}

/// Unregisters `old_key` (if any) and registers `new_key` as a global
/// shortcut, returning the parsed `Shortcut` on success so the caller can
/// remember which physical key combo it now corresponds to. Shared by both
/// the push-to-talk key and the mic-toggle key (see `AppState::ptt_shortcut`
/// / `mic_toggle_shortcut`) — `tauri_plugin_global_shortcut` dispatches every
/// registered shortcut through the one `.with_handler` closure in `lib.rs`,
/// which tells them apart by comparing the fired `Shortcut` against these
/// two remembered values.
pub fn register_shortcut<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    old_key: Option<&str>,
    new_key: &str,
) -> Option<Shortcut> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    if let Some(old) = old_key {
        let old_trimmed = old.trim();
        if !old_trimmed.is_empty() {
            if let Ok(shortcut) = old_trimmed.parse::<Shortcut>() {
                let _ = app.global_shortcut().unregister(shortcut);
            }
        }
    }

    let new_trimmed = new_key.trim();
    if new_trimmed.is_empty() {
        return None;
    }
    let Ok(shortcut) = new_trimmed.parse::<Shortcut>() else {
        tracing::warn!(key = %new_trimmed, "failed to parse key as a valid global shortcut");
        return None;
    };
    if let Err(e) = app.global_shortcut().register(shortcut) {
        tracing::warn!(key = %new_trimmed, error = %e, "failed to register global shortcut");
        return None;
    }
    tracing::info!(key = %new_trimmed, "registered global shortcut");
    Some(shortcut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_manager_muting() {
        let vm = VoiceManager::new();
        assert!(!vm.is_muted("peer1"));
        assert!(vm.muted_peers().is_empty());

        let muted = vm.toggle_mute("peer1");
        assert!(muted);
        assert!(vm.is_muted("peer1"));
        assert_eq!(vm.muted_peers(), vec!["peer1".to_string()]);

        let muted_again = vm.toggle_mute("peer1");
        assert!(!muted_again);
        assert!(!vm.is_muted("peer1"));
        assert!(vm.muted_peers().is_empty());
    }

    #[test]
    fn test_voice_manager_self_mute() {
        let vm = VoiceManager::new();
        // The mic starts off by default -- the user must explicitly turn it
        // on (toggle key or the overlay's mic button).
        assert!(vm.is_self_mic_muted());
        vm.set_self_mic_muted(false);
        assert!(!vm.is_self_mic_muted());
        vm.set_self_mic_muted(true);
        assert!(vm.is_self_mic_muted());
    }
}
