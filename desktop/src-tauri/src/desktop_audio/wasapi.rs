//! Windows process-loopback capture: everything the PC plays EXCEPT one
//! process tree (Warlay's own, so the other players' voices -- which play
//! inside our webview -- are never sent back to them).
//!
//! `ActivateAudioInterfaceAsync` with `VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK`
//! and `PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE`; needs Windows 10
//! build 20348 / Windows 11. Capture runs on its own thread (COM
//! multi-threaded apartment) and hands 48 kHz stereo 16-bit PCM to a callback.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use windows::core::{implement, Interface, Ref, Result as WinResult, HRESULT};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
    IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient, AUDCLNT_BUFFERFLAGS_SILENT,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, AUDIOCLIENT_ACTIVATION_PARAMS,
    AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
    AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE,
    VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX, WAVE_FORMAT_PCM,
};
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, BLOB, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;

use super::encoder::{CHANNELS, SAMPLE_RATE};

/// Signals the capture thread when WASAPI has finished activating the client.
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct ActivationDone(mpsc::Sender<()>);

impl IActivateAudioInterfaceCompletionHandler_Impl for ActivationDone_Impl {
    fn ActivateCompleted(&self, _operation: Ref<'_, IActivateAudioInterfaceAsyncOperation>) -> WinResult<()> {
        let _ = self.0.send(());
        Ok(())
    }
}

/// Stops the capture (and joins its thread) when dropped.
pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Starts capturing everything except `exclude_pid`'s process tree. Errors
/// during activation (unsupported Windows, no output device) are returned
/// here; `on_pcm` then receives interleaved stereo i16 until the handle drops.
pub fn start(exclude_pid: u32, on_pcm: impl FnMut(&[i16]) + Send + 'static) -> Result<CaptureHandle, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let thread_stop = stop.clone();
    let thread = std::thread::Builder::new()
        .name("desktop-audio-capture".into())
        .spawn(move || {
            let outcome = unsafe { run(exclude_pid, &thread_stop, &ready_tx, on_pcm) };
            if let Err(message) = outcome {
                let _ = ready_tx.send(Err(message)); // ignored if already reported
            }
        })
        .map_err(|e| format!("no se pudo crear el hilo de captura: {e}"))?;

    match ready_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => Ok(CaptureHandle {
            stop,
            thread: Some(thread),
        }),
        Ok(Err(message)) => {
            let _ = thread.join();
            Err(message)
        }
        Err(_) => {
            stop.store(true, Ordering::SeqCst);
            Err("la captura de audio del escritorio no respondió".to_string())
        }
    }
}

fn describe(what: &str, error: windows::core::Error) -> String {
    format!("{what}: {error}")
}

unsafe fn run(
    exclude_pid: u32,
    stop: &AtomicBool,
    ready: &mpsc::Sender<Result<(), String>>,
    mut on_pcm: impl FnMut(&[i16]),
) -> Result<(), String> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| describe("COM", e))?;
        let result = capture_loop(exclude_pid, stop, ready, &mut on_pcm);
        CoUninitialize();
        result
    }
}

unsafe fn capture_loop(
    exclude_pid: u32,
    stop: &AtomicBool,
    ready: &mpsc::Sender<Result<(), String>>,
    on_pcm: &mut impl FnMut(&[i16]),
) -> Result<(), String> {
    unsafe {
        // The activation parameters travel as a PROPVARIANT blob.
        let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                    TargetProcessId: exclude_pid,
                    ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE,
                },
            },
        };
        let mut variant = PROPVARIANT::default();
        variant.Anonymous = PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    blob: BLOB {
                        cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                        pBlobData: &mut params as *mut _ as *mut u8,
                    },
                },
            }),
        };
        // `PROPVARIANT` frees a VT_BLOB with CoTaskMemFree when dropped; ours
        // points at the stack (`params`), so it must never be dropped.
        let variant = std::mem::ManuallyDrop::new(variant);

        let (done_tx, done_rx) = mpsc::channel();
        let handler: IActivateAudioInterfaceCompletionHandler = ActivationDone(done_tx).into();
        let operation = ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&*variant),
            &handler,
        )
        .map_err(|e| describe("ActivateAudioInterfaceAsync (¿Windows anterior a la compilación 20348?)", e))?;
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "la activación del audio del escritorio no terminó".to_string())?;

        let mut activate_result = HRESULT(0);
        let mut unknown = None;
        operation
            .GetActivateResult(&mut activate_result, &mut unknown)
            .map_err(|e| describe("GetActivateResult", e))?;
        activate_result
            .ok()
            .map_err(|e| describe("activación del cliente de audio", e))?;
        let client: IAudioClient = unknown
            .ok_or("WASAPI no devolvió un cliente de audio")?
            .cast()
            .map_err(|e| describe("IAudioClient", e))?;

        let format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM as u16,
            nChannels: CHANNELS as u16,
            nSamplesPerSec: SAMPLE_RATE,
            nAvgBytesPerSec: SAMPLE_RATE * CHANNELS as u32 * 2,
            nBlockAlign: (CHANNELS * 2) as u16,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK
                    | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                200_000, // 20 ms buffer, in 100 ns units
                0,
                &format,
                None,
            )
            .map_err(|e| describe("Initialize", e))?;

        let event: HANDLE = CreateEventW(None, false, false, None).map_err(|e| describe("CreateEventW", e))?;
        client.SetEventHandle(event).map_err(|e| describe("SetEventHandle", e))?;
        let capture: IAudioCaptureClient = client.GetService().map_err(|e| describe("IAudioCaptureClient", e))?;
        client.Start().map_err(|e| describe("Start", e))?;
        let _ = ready.send(Ok(()));

        while !stop.load(Ordering::SeqCst) {
            // Wake on data, or every 100 ms to notice `stop`.
            if WaitForSingleObject(event, 100) != WAIT_OBJECT_0 {
                continue;
            }
            loop {
                let available = capture.GetNextPacketSize().unwrap_or(0);
                if available == 0 {
                    break;
                }
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                if capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                    .is_err()
                {
                    break;
                }
                let samples = frames as usize * CHANNELS;
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    on_pcm(&vec![0i16; samples]);
                } else {
                    on_pcm(std::slice::from_raw_parts(data as *const i16, samples));
                }
                let _ = capture.ReleaseBuffer(frames);
            }
        }
        let _ = client.Stop();
        let _ = CloseHandle(event);
        Ok(())
    }
}
