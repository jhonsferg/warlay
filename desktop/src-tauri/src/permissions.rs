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

/// Installs, on the main WebView2, the handler that answers the microphone
/// request from Warlay's own consent AND forbids WebView2 from remembering the
/// answer. WebView2 saves whatever it is told in its profile and, from then on,
/// never asks again: a stored "block" (e.g. from the old default prompt) would
/// leave voice dead for good, and a stored "allow" would survive the user
/// withdrawing their consent. With `SavesInProfile = false` the decision is
/// taken afresh from the settings every time.
#[cfg(windows)]
pub fn install_native_policy(window: &tauri::WebviewWindow, app: tauri::AppHandle) -> tauri::Result<()> {
    use tauri::Manager;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2PermissionRequestedEventArgs3, COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
        COREWEBVIEW2_PERMISSION_STATE_ALLOW, COREWEBVIEW2_PERMISSION_STATE_DENY,
    };
    use webview2_com::PermissionRequestedEventHandler;
    use windows_core::Interface;

    window.with_webview(move |platform| unsafe {
        let Ok(core) = platform.controller().CoreWebView2() else {
            tracing::warn!("no se pudo obtener CoreWebView2 para la politica de permisos");
            return;
        };
        let handler = PermissionRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
            args.PermissionKind(&mut kind)?;
            if kind != COREWEBVIEW2_PERMISSION_KIND_MICROPHONE {
                return Ok(());
            }
            let consent = app
                .try_state::<crate::state::AppState>()
                .and_then(|state| state.store.get_settings().ok())
                .is_some_and(|settings| settings.mic_consent);
            let response = decide(PermissionKind::Microphone, consent);
            tracing::info!(consent, ?response, "microfono solicitado (politica nativa, sin guardar en el perfil)");
            match response {
                PermissionResponse::Allow => args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?,
                PermissionResponse::Deny => args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?,
                _ => {}
            }
            if let Ok(extended) = args.cast::<ICoreWebView2PermissionRequestedEventArgs3>() {
                extended.SetSavesInProfile(false)?;
            }
            Ok(())
        }));
        let mut token = Default::default();
        if let Err(error) = core.add_PermissionRequested(&handler, &mut token) {
            tracing::warn!(%error, "no se pudo instalar la politica nativa de permisos");
        }
    })
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
