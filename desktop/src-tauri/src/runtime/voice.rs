//! Voice frames between the webview and the mesh. The webview captures,
//! encodes (Opus) and plays; this side only carries opaque frames. Inbound
//! frames reach the UI through a Tauri `Channel` as one binary message per
//! batch: no JSON and no base64, so each frame costs a copy instead of a
//! text round trip on the webview's main thread.
//!
//! Wire format of a batch (the webview's `decodeFrameBatch` reads the same):
//! for every frame, the sender's client id (16 bytes), the payload length
//! (u16, little endian) and the payload.

use std::sync::{Arc, Mutex};

use tauri::ipc::{Channel, InvokeResponseBody};
use uuid::Uuid;

/// Which stream of frames a channel receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameStream {
    /// Members' voice (mono).
    Voice,
    /// Other members' shared desktop audio (stereo), apart from their voice.
    Desktop,
}

impl FrameStream {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "voice" => Some(Self::Voice),
            "desktop" => Some(Self::Desktop),
            _ => None,
        }
    }
}

/// The mesh outgoing frames go to. Kept here, apart from the runtime state, so
/// sending a frame (50 times a second per talker) never waits on the lock that
/// every room, tunnel and LAN operation shares.
static VOICE_MESH: Mutex<Option<Arc<warlay::engine::PeerMeshHandle>>> = Mutex::new(None);

/// Sets (or clears, at the end of a room session) the mesh voice is sent to.
pub fn set_voice_mesh(mesh: Option<Arc<warlay::engine::PeerMeshHandle>>) {
    *VOICE_MESH.lock().unwrap_or_else(|e| e.into_inner()) = mesh;
}

/// Sends one encoded voice frame to every connected member; how many it
/// reached (0 when alone or with no room session).
pub fn send_voice_frame_now(frame: &[u8]) -> usize {
    let mesh = VOICE_MESH.lock().unwrap_or_else(|e| e.into_inner()).clone();
    mesh.map_or(0, |mesh| mesh.broadcast_voice_frame(frame))
}

static VOICE_SINK: Mutex<Option<Channel<InvokeResponseBody>>> = Mutex::new(None);
static DESKTOP_SINK: Mutex<Option<Channel<InvokeResponseBody>>> = Mutex::new(None);

fn sink(stream: FrameStream) -> &'static Mutex<Option<Channel<InvokeResponseBody>>> {
    match stream {
        FrameStream::Voice => &VOICE_SINK,
        FrameStream::Desktop => &DESKTOP_SINK,
    }
}

/// Where this stream's batches go from now on (`None` stops them).
pub fn set_sink(stream: FrameStream, channel: Option<Channel<InvokeResponseBody>>) {
    *sink(stream).lock().unwrap_or_else(|e| e.into_inner()) = channel;
}

/// Packs frames into one binary message.
pub fn encode_batch(frames: &[(Uuid, Vec<u8>)]) -> Vec<u8> {
    let size = frames.iter().map(|(_, data)| 18 + data.len()).sum();
    let mut out = Vec::with_capacity(size);
    for (from, data) in frames {
        // A datagram is far smaller than 64 KiB; anything longer is cut, not
        // allowed to corrupt the framing of the frames after it.
        let len = data.len().min(u16::MAX as usize);
        out.extend_from_slice(from.as_bytes());
        out.extend_from_slice(&(len as u16).to_le_bytes());
        out.extend_from_slice(&data[..len]);
    }
    out
}

fn emit(stream: FrameStream, frames: Vec<(Uuid, Vec<u8>)>) {
    if frames.is_empty() {
        return;
    }
    let mut guard = sink(stream).lock().unwrap_or_else(|e| e.into_inner());
    let Some(channel) = guard.as_ref() else {
        return;
    };
    // A closed channel (the webview reloaded) is dropped; the UI registers a
    // new one when it comes back.
    if channel.send(InvokeResponseBody::Raw(encode_batch(&frames))).is_err() {
        *guard = None;
    }
}

pub fn emit_voice_frames(frames: Vec<(Uuid, Vec<u8>)>) {
    emit(FrameStream::Voice, frames);
}

pub fn emit_desktop_frames(frames: Vec<(Uuid, Vec<u8>)>) {
    emit(FrameStream::Desktop, frames);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_names_map_to_their_stream() {
        assert_eq!(FrameStream::parse("voice"), Some(FrameStream::Voice));
        assert_eq!(FrameStream::parse("desktop"), Some(FrameStream::Desktop));
        assert_eq!(FrameStream::parse("other"), None);
    }

    #[test]
    fn a_batch_keeps_sender_order_and_payloads() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let bytes = encode_batch(&[(a, vec![1, 2, 3]), (b, vec![]), (a, vec![255])]);
        assert_eq!(bytes.len(), (18 + 3) + 18 + (18 + 1));

        assert_eq!(&bytes[0..16], a.as_bytes());
        assert_eq!(&bytes[16..18], &[3, 0]);
        assert_eq!(&bytes[18..21], &[1, 2, 3]);
        assert_eq!(&bytes[21..37], b.as_bytes());
        assert_eq!(&bytes[37..39], &[0, 0]);
        assert_eq!(&bytes[39..55], a.as_bytes());
        assert_eq!(&bytes[55..57], &[1, 0]);
        assert_eq!(bytes[57], 255);
    }

    #[test]
    fn an_empty_batch_is_empty() {
        assert!(encode_batch(&[]).is_empty());
    }

    #[test]
    fn an_oversized_payload_is_cut_without_breaking_the_next_frame() {
        let a = Uuid::new_v4();
        let bytes = encode_batch(&[(a, vec![7; 70_000]), (a, vec![9])]);
        assert_eq!(bytes.len(), (18 + 65_535) + 19);
        let tail = &bytes[18 + 65_535..];
        assert_eq!(&tail[..16], a.as_bytes());
        assert_eq!(&tail[16..18], &[1, 0]);
        assert_eq!(tail[18], 9);
    }
}
