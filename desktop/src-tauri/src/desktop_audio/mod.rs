//! Sharing the PC's own sound (game, music, a browser) with the room:
//! captured by Windows process-loopback (excluding Warlay itself), encoded to
//! Opus and sent over the mesh as its own stream. See `wasapi` and `encoder`.

pub mod encoder;
#[cfg(windows)]
pub mod wasapi;

use std::sync::Arc;

use warlay::engine::PeerMeshHandle;

/// A running share. Dropping it stops the capture.
pub struct DesktopAudioShare {
    #[cfg(windows)]
    _capture: wasapi::CaptureHandle,
}

/// Starts sharing: captures everything but this process tree, encodes it, and
/// broadcasts each packet to the room through `mesh`.
#[cfg(windows)]
pub fn start(mesh: Arc<PeerMeshHandle>) -> Result<DesktopAudioShare, String> {
    let mut encoder = encoder::DesktopEncoder::new()?;
    let capture = wasapi::start(std::process::id(), move |pcm| {
        encoder.push(pcm, |packet| {
            mesh.broadcast_desktop_frame(packet);
        });
    })?;
    Ok(DesktopAudioShare { _capture: capture })
}

#[cfg(not(windows))]
pub fn start(_mesh: Arc<PeerMeshHandle>) -> Result<DesktopAudioShare, String> {
    Err("compartir el audio del escritorio solo está disponible en Windows".to_string())
}

#[cfg(all(test, windows))]
#[path = "wasapi_tests.rs"]
mod wasapi_tests;
