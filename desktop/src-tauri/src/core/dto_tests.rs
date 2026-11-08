use super::*;

/// What an installation that predates the voice settings has stored: none of
/// the new keys. It must still load as a whole, or `get_settings` falls back
/// to factory defaults and wipes the user's name, executable path, etc.
const LEGACY_SETTINGS_JSON: &str = r#"{
    "displayName": "Arthas",
    "gameExecutablePath": "C:\\Games\\War3.exe",
    "gameArgs": "-window",
    "gameVersion": 126,
    "slotsTotal": 10,
    "consoleLogLevel": "info",
    "fileLogLevel": "debug",
    "chatMessageLimit": 100
}"#;

#[test]
fn settings_saved_before_the_voice_options_still_load_whole() {
    let settings: AppSettings = serde_json::from_str(LEGACY_SETTINGS_JSON).expect("legacy json loads");
    assert_eq!(settings.display_name, "Arthas");
    assert_eq!(settings.game_executable_path, "C:\\Games\\War3.exe");
    assert_eq!(settings.slots_total, 10);
}

#[test]
fn voice_options_default_to_asking_first_and_standard_noise_suppression() {
    let settings: AppSettings = serde_json::from_str(LEGACY_SETTINGS_JSON).unwrap();
    assert!(!settings.mic_consent, "never assume consent for the microphone");
    assert_eq!(settings.mic_device_id, "");
    assert_eq!(settings.noise_suppression, "standard");
    assert!(!settings.share_desktop_audio, "sharing the desktop sound is opt-in");
}

#[test]
fn voice_options_round_trip_with_camel_case_keys() {
    let mut settings: AppSettings = serde_json::from_str(LEGACY_SETTINGS_JSON).unwrap();
    settings.mic_consent = true;
    settings.mic_device_id = "abc123".into();
    settings.noise_suppression = "strong".into();
    settings.share_desktop_audio = true;

    let json = serde_json::to_value(&settings).unwrap();
    assert_eq!(json["micConsent"], true);
    assert_eq!(json["micDeviceId"], "abc123");
    assert_eq!(json["noiseSuppression"], "strong");
    assert_eq!(json["shareDesktopAudio"], true);

    let back: AppSettings = serde_json::from_value(json).unwrap();
    assert!(back.mic_consent);
    assert_eq!(back.noise_suppression, "strong");
    assert!(back.share_desktop_audio);
}
