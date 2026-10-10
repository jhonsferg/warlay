use crate::daemon_ipc::DaemonClient;
use crate::voice::codec::{create_encoder, SAMPLES_PER_FRAME};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use warlay::protocol::ipc::DaemonRequest;

pub struct AudioCapture {
    running: Arc<AtomicBool>,
    /// Whether this machine's own mic is currently allowed to transmit --
    /// the *only* gate on sending audio (see `process_samples` below).
    /// Starts (and is reset on every `start()`) to `true` -- the mic is
    /// off by default until the user's configured toggle key or the
    /// overlay's mic button turns it on. There's no separate hold-to-talk
    /// key anymore: one toggle, on or off, is the whole model.
    self_muted: Arc<AtomicBool>,
    _stream: Option<Stream>,
}

// `cpal::Stream` is deliberately `!Send`/`!Sync` (see cpal's own
// `NotSendSyncAcrossAllPlatforms` marker) since some backends could in
// principle have real thread-affinity constraints. In practice the
// `Stream` value itself is just a handle/lifetime-guard -- the actual
// audio-callback work happens on cpal's own dedicated thread regardless
// of which thread holds this handle. Safe here specifically because
// `AudioCapture` is only ever reached through `VoiceManager`'s
// `Arc<Mutex<AudioCapture>>` (see voice/mod.rs), which already
// serializes every access -- nothing calls stream methods concurrently
// from multiple threads. Needed because `AudioCapture` lives inside
// Tauri's shared `AppState`, which requires `Send + Sync` across async
// command boundaries.
unsafe impl Send for AudioCapture {}
unsafe impl Sync for AudioCapture {}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            self_muted: Arc::new(AtomicBool::new(true)),
            _stream: None,
        }
    }

    pub fn set_self_muted(&self, muted: bool) {
        self.self_muted.store(muted, Ordering::SeqCst);
        tracing::info!(muted, "mic toggled");
    }

    pub fn is_self_muted(&self) -> bool {
        self.self_muted.load(Ordering::SeqCst)
    }

    pub fn start(
        &mut self,
        room_token: String,
        daemon_client: DaemonClient,
        device_id: Option<String>,
    ) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Every voice session starts with the mic off, regardless of
        // whatever it was left at last time -- the user must explicitly
        // turn it on (toggle key or the overlay's mic button) each time.
        self.self_muted.store(true, Ordering::SeqCst);

        let host = cpal::default_host();
        let device = match device_id {
            Some(id) => {
                let mut devices = host.input_devices().map_err(|e| e.to_string())?;
                devices
                    .find(|d| d.name().map(|n| n == id).unwrap_or(false))
                    .ok_or_else(|| "specified input device not found".to_string())?
            }
            None => host
                .default_input_device()
                .ok_or_else(|| "no default input device available".to_string())?,
        };

        let config_fmt = device.default_input_config().map_err(|e| e.to_string())?;
        let sample_rate = config_fmt.sample_rate().0;
        let channels = config_fmt.channels() as usize;
        let sample_format = config_fmt.sample_format();

        // Voice chat had no logging at all before this -- when it silently
        // never transmitted (mic off by default, or `voiceEnabled` false so
        // this never even runs), there was nothing in the log to tell the
        // two apart. This one line at least confirms capture itself did
        // start, with what device/format.
        tracing::info!(
            device = %device.name().unwrap_or_default(),
            sample_rate,
            channels,
            ?sample_format,
            "voice capture starting (mic is off until toggled on)"
        );

        let encoder = create_encoder().map_err(|e| format!("failed to create Opus encoder: {}", e))?;

        let running = Arc::new(AtomicBool::new(true));
        self.running = running.clone();

        let seq_counter = Arc::new(AtomicU32::new(0));
        let pcm_buffer = Arc::new(Mutex::new(Vec::<i16>::new()));

        let err_fn = |err| {
            tracing::error!("CPAL input stream error: {}", err);
        };

        let stream_config = config_fmt.config();

        let encoder = Arc::new(Mutex::new(encoder));
        let self_muted = self.self_muted.clone();

        let process_samples = move |mono_i16_samples: &[i16]| {
            if !running.load(Ordering::SeqCst) || self_muted.load(Ordering::SeqCst) {
                return;
            }

            let mut buf = match pcm_buffer.try_lock() {
                Ok(b) => b,
                Err(_) => return,
            };

            // Resample to 48kHz if needed
            if sample_rate == 48000 {
                buf.extend_from_slice(mono_i16_samples);
            } else {
                let step = sample_rate as f32 / 48000.0;
                let mut idx = 0.0f32;
                while (idx as usize) < mono_i16_samples.len() {
                    buf.push(mono_i16_samples[idx as usize]);
                    idx += step;
                }
            }

            while buf.len() >= SAMPLES_PER_FRAME {
                let frame: Vec<i16> = buf.drain(0..SAMPLES_PER_FRAME).collect();

                let encoder_guard = match encoder.try_lock() {
                    Ok(g) => g,
                    Err(_) => break,
                };

                let mut opus_out = vec![0u8; 512];
                if let Ok(encoded_len) = encoder_guard.encode(&frame, &mut opus_out) {
                    let seq = seq_counter.fetch_add(1, Ordering::SeqCst);
                    let mut payload = Vec::with_capacity(4 + encoded_len);
                    payload.extend_from_slice(&seq.to_be_bytes());
                    payload.extend_from_slice(&opus_out[..encoded_len]);

                    let client = daemon_client.clone();
                    let token = room_token.clone();

                    // `process_samples` runs on cpal's own dedicated audio
                    // callback thread, not a tokio worker -- plain
                    // `tokio::spawn` relies on ambient thread-local runtime
                    // context that thread never has, and panics immediately.
                    // `tauri::async_runtime::spawn` reaches into Tauri's own
                    // globally-registered runtime instead (same pattern
                    // already used in lib.rs/log_cleanup.rs for this exact
                    // reason).
                    tauri::async_runtime::spawn(async move {
                        let _ = client
                            .call(DaemonRequest::VoiceFrameOut {
                                room_token: token,
                                opus_bytes: payload,
                            })
                            .await;
                    });
                }
            }
        };

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    let mono = to_mono_i16_f32(data, channels);
                    process_samples(&mono);
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    let mono = to_mono_i16(data, channels);
                    process_samples(&mono);
                },
                err_fn,
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    let mono = to_mono_i16_u16(data, channels);
                    process_samples(&mono);
                },
                err_fn,
                None,
            ),
            _ => return Err("unsupported audio sample format".to_string()),
        }
        .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;
        self._stream = Some(stream);

        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self._stream = None;
    }
}

fn to_mono_i16_f32(data: &[f32], channels: usize) -> Vec<i16> {
    if channels == 1 {
        data.iter()
            .map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16)
            .collect()
    } else {
        data.chunks(channels)
            .map(|chunk| {
                let sum: f32 = chunk.iter().sum();
                let avg = sum / channels as f32;
                (avg * 32767.0).clamp(-32768.0, 32767.0) as i16
            })
            .collect()
    }
}

fn to_mono_i16(data: &[i16], channels: usize) -> Vec<i16> {
    if channels == 1 {
        data.to_vec()
    } else {
        data.chunks(channels)
            .map(|chunk| {
                let sum: i32 = chunk.iter().map(|&s| s as i32).sum();
                (sum / channels as i32) as i16
            })
            .collect()
    }
}

fn to_mono_i16_u16(data: &[u16], channels: usize) -> Vec<i16> {
    if channels == 1 {
        data.iter().map(|&s| (s as i32 - 32768) as i16).collect()
    } else {
        data.chunks(channels)
            .map(|chunk| {
                let sum: i32 = chunk.iter().map(|&s| s as i32 - 32768).sum();
                (sum / channels as i32) as i16
            })
            .collect()
    }
}
