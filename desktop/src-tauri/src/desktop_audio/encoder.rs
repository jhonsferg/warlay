//! Interleaved 48 kHz stereo PCM in, 20 ms Opus packets out.
//!
//! The capture hands over PCM in whatever chunk sizes WASAPI delivers; Opus
//! needs exact 20 ms frames, so this buffers the remainder between calls.
//! Frames that are pure digital silence are not sent at all (nothing playing
//! on the PC costs no bandwidth); the receiver simply hears nothing.

use audiopus::coder::Encoder;
use audiopus::{Application, Bitrate, Channels, SampleRate};

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
/// 20 ms of stereo audio, in samples (not frames).
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE as usize / 50) * CHANNELS;
/// Target bitrate: music/game audio, stereo.
const BITRATE: i32 = 64_000;
/// Larger than any packet Opus can produce for one 20 ms frame at this rate.
const MAX_PACKET: usize = 1275;

pub struct DesktopEncoder {
    encoder: Encoder,
    pending: Vec<i16>,
    packet: [u8; MAX_PACKET],
}

impl DesktopEncoder {
    pub fn new() -> Result<Self, String> {
        let mut encoder = Encoder::new(SampleRate::Hz48000, Channels::Stereo, Application::Audio)
            .map_err(|e| format!("no se pudo crear el codificador Opus: {e}"))?;
        encoder
            .set_bitrate(Bitrate::BitsPerSecond(BITRATE))
            .map_err(|e| format!("bitrate Opus inválido: {e}"))?;
        Ok(Self {
            encoder,
            pending: Vec::with_capacity(FRAME_SAMPLES * 2),
            packet: [0; MAX_PACKET],
        })
    }

    /// Feeds interleaved stereo samples; calls `emit` once per finished,
    /// non-silent 20 ms packet. Returns the number of packets emitted.
    pub fn push(&mut self, pcm: &[i16], mut emit: impl FnMut(&[u8])) -> usize {
        self.pending.extend_from_slice(pcm);
        let mut emitted = 0;
        let mut consumed = 0;
        while self.pending.len() - consumed >= FRAME_SAMPLES {
            let frame = &self.pending[consumed..consumed + FRAME_SAMPLES];
            consumed += FRAME_SAMPLES;
            if frame.iter().all(|&s| s == 0) {
                continue;
            }
            if let Ok(len) = self.encoder.encode(frame, &mut self.packet) {
                emit(&self.packet[..len]);
                emitted += 1;
            }
        }
        self.pending.drain(..consumed);
        emitted
    }

    /// Samples waiting for the rest of their 20 ms frame.
    #[cfg(test)]
    pub fn buffered(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
#[path = "encoder_tests.rs"]
mod tests;
