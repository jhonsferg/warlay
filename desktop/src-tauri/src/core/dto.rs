use serde::{Deserialize, Serialize};
use warlay::protocol::{RoomSummary, ServerMessage};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDto {
    pub source_addr: String,
    pub game_name: Option<String>,
    pub last_seen_secs_ago: u64,
    pub payload_len: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfoDto {
    pub cpu_cores: u32,
    pub total_ram_mb: u64,
    pub gpu_name: Option<String>,
    pub gpu_vram_mb: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResultDto {
    pub pid: u32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RelayStatusDto {
    pub running: bool,
    pub peers: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerDto {
    pub id: String,
    pub name: String,
    pub address: String,
    pub server_id: Option<String>,
    pub has_credentials: bool,
    pub last_connected_at: Option<u64>,
}

#[derive(Serialize, Clone)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ConnectStatusDto {
    Connected { server_name: String },
    PasswordRequired { server_name: String },
    Rejected { message: String, retry_as_new: bool },
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatusEvent {
    pub server_id: String,
    pub connected: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ServerPingEvent {
    pub server_id: String,
    pub ping_ms: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ServerMessageEvent {
    pub server_id: String,
    pub message: ServerMessage,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VoiceFrameEvent {
    pub from_client_id: String,
    pub opus_bytes: Vec<u8>,
}

fn default_master_volume() -> f32 {
    1.0
}

fn default_log_retention_days() -> u32 {
    7
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub display_name: String,
    pub game_executable_path: String,
    pub game_args: String,
    pub game_version: u32,
    pub slots_total: u32,
    pub console_log_level: String,
    pub file_log_level: String,
    pub chat_message_limit: u32,
    // "" means no `-graphicsapi` flag is appended at launch (whatever the
    // game picks on its own); otherwise one of "Direct3D9" / "Direct3D11" /
    // "OpenGL2", passed through verbatim as WC3 expects it capitalized. Not
    // validated here -- the frontend's <select> is the only writer, so an
    // unexpected value can only come from someone hand-editing the DB.
    #[serde(default)]
    pub graphics_api: String,
    // `#[serde(default = ...)]` on both fields matters: without it, an
    // existing user's settings row (saved before this field existed) would
    // fail to deserialize as `AppSettings` entirely, and `get_settings`
    // silently falls back to factory defaults -- wiping their display name,
    // executable path, etc, not just the audio prefs.
    #[serde(default = "default_master_volume")]
    pub master_volume: f32,
    #[serde(default)]
    pub muted: bool,
    // "" plays through the OS default output device. The value is a
    // `MediaDeviceInfo.deviceId` from the frontend's `enumerateDevices()`
    // call -- opaque here, only meaningful to the browser engine that
    // issued it (see `AudioSettingsSection.vue`).
    #[serde(default)]
    pub audio_output_device_id: String,
    // See `log_cleanup` -- log files older than this many days are deleted
    // once at every launch, and again immediately whenever this changes.
    #[serde(default = "default_log_retention_days")]
    pub log_retention_days: u32,
    #[serde(default)]
    pub voice_enabled: bool,
    #[serde(default)]
    pub microphone_device_id: String,
    // Toggles the microphone on/off on press -- the *only* voice control
    // key. The mic always starts off (see `voice::capture::AudioCapture`);
    // this is what turns it on, and pressing it again mutes back. See
    // `voice::register_shortcut` and its two call sites.
    #[serde(default)]
    pub mic_toggle_key: String,
    #[serde(default = "default_master_volume")]
    pub voice_input_volume: f32,
    #[serde(default = "default_master_volume")]
    pub voice_output_volume: f32,
}
