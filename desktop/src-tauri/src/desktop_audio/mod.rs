//! Sharing the PC's own sound (game, music, a browser) with the room:
//! captured by Windows process-loopback (excluding Warlay itself), encoded to
//! Opus and sent over the mesh as its own stream. See `wasapi` and `encoder`.

pub mod encoder;
#[cfg(windows)]
pub mod process;
#[cfg(windows)]
pub mod wasapi;

use std::sync::Arc;

use warlay::engine::PeerMeshHandle;

#[cfg(windows)]
pub use process::exclusion_pid;

/// On other platforms nothing is excluded (sharing is Windows-only anyway).
#[cfg(not(windows))]
pub fn exclusion_pid() -> u32 {
    std::process::id()
}

/// A running share. Dropping it stops the capture.
pub struct DesktopAudioShare {
    #[cfg(windows)]
    _capture: wasapi::CaptureHandle,
}

/// Starts sharing: captures everything the PC plays except the process tree of
/// `exclude_pid` (see `process::exclusion_pid`), encodes it, and broadcasts each
/// packet to the room through `mesh`.
#[cfg(windows)]
pub fn start(mesh: Arc<PeerMeshHandle>, exclude_pid: u32) -> Result<DesktopAudioShare, String> {
    let mut encoder = encoder::DesktopEncoder::new()?;
    let capture = wasapi::start(exclude_pid, move |pcm| {
        encoder.push(pcm, |packet| {
            mesh.broadcast_desktop_frame(packet);
        });
    })?;
    Ok(DesktopAudioShare { _capture: capture })
}

#[cfg(not(windows))]
pub fn start(_mesh: Arc<PeerMeshHandle>, _exclude_pid: u32) -> Result<DesktopAudioShare, String> {
    Err("compartir el audio del escritorio solo está disponible en Windows".to_string())
}

#[cfg(all(test, windows))]
#[path = "wasapi_tests.rs"]
mod wasapi_tests;
#[cfg(all(test, windows))]
#[path = "mesh_e2e_tests.rs"]
mod mesh_e2e_tests;
