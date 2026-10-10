use crate::voice::codec::{create_decoder, SAMPLES_PER_FRAME};
use crate::voice::jitter::{JitterBufferManager, DEFAULT_TARGET_DEPTH};
use audiopus::coder::Decoder;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

pub struct AudioPlayback {
    running: Arc<AtomicBool>,
    _stream: Option<Stream>,
    state: Arc<Mutex<PlaybackState>>,
}

// Same reasoning as `AudioCapture` in capture.rs: `cpal::Stream` is
// `!Send`/`!Sync` by design, but `AudioPlayback` is only ever reached
// through `VoiceManager`'s `Arc<Mutex<AudioPlayback>>`, which already
// serializes access -- and it needs to live in Tauri's shared `AppState`.
unsafe impl Send for AudioPlayback {}
unsafe impl Sync for AudioPlayback {}

struct PlaybackState {
    jitter_mgr: JitterBufferManager,
    decoders: HashMap<String, Decoder>,
}

impl AudioPlayback {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            _stream: None,
            state: Arc::new(Mutex::new(PlaybackState {
                jitter_mgr: JitterBufferManager::new(DEFAULT_TARGET_DEPTH),
                decoders: HashMap::new(),
            })),
        }
    }

    pub fn start(&mut self, device_id: Option<String>) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        let host = cpal::default_host();
        let device = match device_id {
            Some(id) => {
                let mut devices = host.output_devices().map_err(|e| e.to_string())?;
                devices
                    .find(|d| d.name().map(|n| n == id).unwrap_or(false))
                    .ok_or_else(|| "specified output device not found".to_string())?
            }
            None => host
                .default_output_device()
                .ok_or_else(|| "no default output device available".to_string())?,
        };

        let config_fmt = device.default_output_config().map_err(|e| e.to_string())?;
        let channels = config_fmt.channels() as usize;
        let sample_format = config_fmt.sample_format();
        let stream_config = config_fmt.config();

        let running = Arc::new(AtomicBool::new(true));
        self.running = running.clone();

        let state = self.state.clone();
        let err_fn = |err| {
            tracing::error!("CPAL output stream error: {}", err);
        };

        let stream = match sample_format {
            SampleFormat::F32 => device.build_output_stream(
                &stream_config,
                move |data: &mut [f32], _| {
                    if !running.load(Ordering::SeqCst) {
                        data.fill(0.0);
                        return;
                    }
                    write_mixed_audio_f32(data, channels, &state);
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => device.build_output_stream(
                &stream_config,
                move |data: &mut [i16], _| {
                    if !running.load(Ordering::SeqCst) {
                        data.fill(0);
                        return;
                    }
                    write_mixed_audio_i16(data, channels, &state);
                },
                err_fn,
                None,
            ),
            _ => return Err("unsupported output audio sample format".to_string()),
        }
        .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;
        self._stream = Some(stream);

        Ok(())
    }

    pub fn push_frame(&self, from_client_id: String, opus_bytes: Vec<u8>) {
        if opus_bytes.len() < 4 {
            return;
        }

        let mut seq_bytes = [0u8; 4];
        seq_bytes.copy_from_slice(&opus_bytes[0..4]);
        let seq = u32::from_be_bytes(seq_bytes);
        let payload = opus_bytes[4..].to_vec();

        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };

        state.jitter_mgr.push_frame(&from_client_id, seq, payload);
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self._stream = None;
        if let Ok(mut state) = self.state.lock() {
            state.decoders.clear();
            // Otherwise frames buffered right before a stop (e.g. leaving a
            // room) would still be sitting in each peer's jitter buffer the
            // next time playback starts, and get mixed in as a burst of
            // stale audio from a previous session.
            state.jitter_mgr = JitterBufferManager::new(DEFAULT_TARGET_DEPTH);
        }
    }
}

fn mix_next_frame(state: &Arc<Mutex<PlaybackState>>) -> Vec<i16> {
    let mut state = match state.lock() {
        Ok(s) => s,
        Err(_) => return vec![0i16; SAMPLES_PER_FRAME],
    };

    let active_peers = state.jitter_mgr.active_peers();
    let mut decoded_peer_frames = Vec::new();

    for peer_id in active_peers {
        if let Some(opus_payload) = state.jitter_mgr.pop_frame_for_peer(&peer_id) {
            let decoder = state
                .decoders
                .entry(peer_id.clone())
                .or_insert_with(|| create_decoder().expect("failed to create decoder for peer"));

            let mut pcm_buf = vec![0i16; SAMPLES_PER_FRAME];
            if let Ok(decoded_count) = decoder.decode(Some(&opus_payload), &mut pcm_buf, false) {
                if decoded_count == SAMPLES_PER_FRAME {
                    decoded_peer_frames.push(pcm_buf);
                }
            }
        }
    }

    if decoded_peer_frames.is_empty() {
        return vec![0i16; SAMPLES_PER_FRAME];
    }

    let mut mixed = vec![0i16; SAMPLES_PER_FRAME];
    for i in 0..SAMPLES_PER_FRAME {
        let mut sum: i32 = 0;
        for frame in &decoded_peer_frames {
            sum += frame[i] as i32;
        }
        mixed[i] = sum.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }

    mixed
}

fn write_mixed_audio_f32(data: &mut [f32], channels: usize, state: &Arc<Mutex<PlaybackState>>) {
    let frame_samples = mix_next_frame(state);
    let mut frame_idx = 0;

    for chunk in data.chunks_mut(channels) {
        let sample_i16 = if frame_idx < frame_samples.len() {
            frame_samples[frame_idx]
        } else {
            0
        };
        let sample_f32 = sample_i16 as f32 / 32768.0;

        for ch in chunk.iter_mut() {
            *ch = sample_f32;
        }
        frame_idx += 1;
    }
}

fn write_mixed_audio_i16(data: &mut [i16], channels: usize, state: &Arc<Mutex<PlaybackState>>) {
    let frame_samples = mix_next_frame(state);
    let mut frame_idx = 0;

    for chunk in data.chunks_mut(channels) {
        let sample_i16 = if frame_idx < frame_samples.len() {
            frame_samples[frame_idx]
        } else {
            0
        };

        for ch in chunk.iter_mut() {
            *ch = sample_i16;
        }
        frame_idx += 1;
    }
}
