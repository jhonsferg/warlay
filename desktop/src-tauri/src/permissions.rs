//! What the webview may do without WebView2's own permission prompt.
//!
//! Microphone access is gated by Warlay's own dialog: the frontend asks the
//! user, stores the answer (`AppSettings::mic_consent`) and only then calls
//! `getUserMedia`. This policy then answers the browser engine's request
//! itself, so the default WebView2 prompt never appears:
//!
//! * consent given   -> Allow
//! * no consent yet  -> Deny (the frontend never reaches this: it asks first)
//!
//! Everything else keeps the engine's default behaviour. Windows' own
//! "let apps use the microphone" switch still applies underneath and can't be
//! bypassed; the frontend detects that case and points the user to it.

use tauri::webview::{PermissionKind, PermissionResponse};

pub fn decide(kind: PermissionKind, mic_consent: bool) -> PermissionResponse {
    match kind {
        PermissionKind::Microphone if mic_consent => PermissionResponse::Allow,
        PermissionKind::Microphone => PermissionResponse::Deny,
        _ => PermissionResponse::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_microphone_is_allowed_only_after_the_user_consented() {
        assert!(matches!(
            decide(PermissionKind::Microphone, true),
            PermissionResponse::Allow
        ));
        assert!(matches!(
            decide(PermissionKind::Microphone, false),
            PermissionResponse::Deny
        ));
    }

    #[test]
    fn consent_for_the_microphone_never_unlocks_anything_else() {
        for kind in [
            PermissionKind::Camera,
            PermissionKind::Geolocation,
            PermissionKind::Notifications,
            PermissionKind::ClipboardRead,
            PermissionKind::Sensors,
            PermissionKind::Other,
        ] {
            assert!(
                matches!(decide(kind, true), PermissionResponse::Default),
                "{kind:?} must stay on the engine default"
            );
        }
    }
}
