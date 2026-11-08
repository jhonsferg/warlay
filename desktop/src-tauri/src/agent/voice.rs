//! Voice frames between the webview and the mesh. The webview captures,
//! encodes (Opus) and plays; this side only carries opaque frames. Inbound
//! frames reach the UI as one batched event per poll tick, base64 so the JSON
//! stays compact.

use base64::Engine;
use serde::Serialize;
use tauri::Emitter;
use uuid::Uuid;

pub const VOICE_EVENT: &str = "voice://frames";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceFrameEvent {
    /// The speaking member's client id.
    pub from: String,
    /// The encoded Opus frame, base64.
    pub data: String,
}

pub fn to_events(frames: Vec<(Uuid, Vec<u8>)>) -> Vec<VoiceFrameEvent> {
    frames
        .into_iter()
        .map(|(from, data)| VoiceFrameEvent {
            from: from.to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(data),
        })
        .collect()
}

pub fn emit_voice_frames(frames: Vec<(Uuid, Vec<u8>)>) {
    if frames.is_empty() {
        return;
    }
    if let Some(app) = super::events::app_handle() {
        let _ = app.emit(VOICE_EVENT, to_events(frames));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_become_base64_events_keeping_sender_and_order() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let events = to_events(vec![(a, vec![1, 2, 3]), (b, vec![]), (a, vec![255])]);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].from, a.to_string());
        assert_eq!(events[0].data, "AQID");
        assert_eq!(events[1].from, b.to_string());
        assert_eq!(events[1].data, "");
        assert_eq!(events[2].data, "/w==");
    }

    #[test]
    fn the_event_serialises_to_the_shape_the_frontend_reads() {
        let id = Uuid::new_v4();
        let json = serde_json::to_value(to_events(vec![(id, vec![0, 0])])).unwrap();
        assert_eq!(json[0]["from"], id.to_string());
        assert_eq!(json[0]["data"], "AAA=");
    }
}
