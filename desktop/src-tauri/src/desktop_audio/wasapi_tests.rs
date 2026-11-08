//! Real-audio checks of the process-loopback capture. They need an audio
//! output device and Windows 11 / 10 build 20348+, so they are `#[ignore]`d;
//! run them on purpose:
//!
//!     cargo test -p desktop -- --ignored --test-threads=1
//!
//! Tones are played by OTHER processes (PowerShell's SoundPlayer) and the
//! capture must hear them -- unless that process, or an ancestor of it, is the
//! one excluded. The machine may be playing other sound too, so tones are
//! detected by frequency (Goertzel), not by overall level.

use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::encoder::{CHANNELS, SAMPLE_RATE};
use super::wasapi;

/// Amplitude (in i16 units) of a pure tone at `hz` inside interleaved stereo
/// `pcm`, by the Goertzel algorithm on the left channel. Other audio playing
/// on the machine has no such pure line, so this tells OUR tone apart from the
/// background, which an overall level cannot.
pub(super) fn tone_amplitude(pcm: &[i16], hz: f64) -> f64 {
    let n = pcm.len() / CHANNELS;
    if n == 0 {
        return 0.0;
    }
    let w = 2.0 * std::f64::consts::PI * hz / SAMPLE_RATE as f64;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for i in 0..n {
        let s0 = pcm[i * CHANNELS] as f64 + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
    2.0 * power.max(0.0).sqrt() / n as f64
}

/// The strongest amplitude of `hz` over any 0.5 s block of `pcm`.
pub(super) fn loudest_tone(pcm: &[i16], hz: f64) -> f64 {
    let half_second = (SAMPLE_RATE as usize / 2) * CHANNELS;
    pcm.chunks(half_second)
        .map(|c| tone_amplitude(c, hz))
        .fold(0.0, f64::max)
}

/// A WAV of a `hz` tone, `seconds` long, 48 kHz stereo 16-bit, half scale.
pub(super) fn tone_wav(seconds: u32, hz: f32) -> std::path::PathBuf {
    let frames = SAMPLE_RATE * seconds;
    let mut data = Vec::with_capacity(frames as usize * 4);
    for i in 0..frames {
        let s = (2.0 * std::f32::consts::PI * hz * i as f32 / SAMPLE_RATE as f32).sin();
        let v = (s * 0.5 * i16::MAX as f32) as i16;
        for _ in 0..CHANNELS {
            data.extend_from_slice(&v.to_le_bytes());
        }
    }
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&(CHANNELS as u16).to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * CHANNELS as u32 * 2).to_le_bytes());
    wav.extend_from_slice(&((CHANNELS * 2) as u16).to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    let path = std::env::temp_dir().join(format!("warlay-tone-{}-{}.wav", std::process::id(), hz as u32));
    std::fs::write(&path, wav).unwrap();
    path
}

fn player_command(wav: &std::path::Path) -> String {
    format!("(New-Object Media.SoundPlayer '{}').PlaySync()", wav.display())
}

/// A direct child of this test that plays `wav`.
pub(super) fn play(wav: &std::path::Path) -> Child {
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &player_command(wav)])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the tone player")
}

/// `cmd` -> `powershell` playing `wav`: the sound comes from a GRANDCHILD of
/// the returned process, to prove that excluding a process excludes its tree.
fn play_under_a_wrapper(wav: &std::path::Path) -> Child {
    Command::new("cmd")
        .args([
            "/C",
            &format!(
                "powershell -NoProfile -NonInteractive -Command \"{}\"",
                player_command(wav)
            ),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the wrapper")
}

/// A process unrelated to the players, only there to be excluded.
pub(super) fn bystander() -> Child {
    Command::new("ping")
        .args(["-n", "60", "127.0.0.1"])
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn a bystander")
}

/// Captures for `window`, excluding `exclude_pid`, and returns the PCM.
fn capture(exclude_pid: u32, window: Duration) -> Vec<i16> {
    let pcm = Arc::new(Mutex::new(Vec::<i16>::new()));
    let sink = pcm.clone();
    let handle = wasapi::start(exclude_pid, move |chunk| sink.lock().unwrap().extend_from_slice(chunk))
        .expect("start the process-loopback capture");
    std::thread::sleep(window);
    drop(handle);
    let all = pcm.lock().unwrap().clone();
    all
}

const AUDIBLE: f64 = 8000.0; // the tone is a half-scale sine: ~16000 at the source
const ABSENT: f64 = 1500.0; // background noise leaks ~1000 into a single bin

#[test]
#[ignore = "needs an audio output device and Windows 11 / build 20348+"]
fn audio_from_another_process_is_captured() {
    let wav = tone_wav(8, 440.0);
    let mut other = bystander();
    let mut player = play(&wav);
    let pcm = capture(other.id(), Duration::from_secs(5));
    let _ = player.kill();
    let _ = other.kill();
    let tone = loudest_tone(&pcm, 440.0);
    println!("440 Hz amplitude (excluding an unrelated process): {tone:.0}");
    assert!(
        tone > AUDIBLE,
        "the tone played by another process was not heard ({tone:.0})"
    );
}

#[test]
#[ignore = "needs an audio output device and Windows 11 / build 20348+"]
fn audio_from_the_excluded_process_is_not_captured() {
    let wav = tone_wav(8, 440.0);
    let mut player = play(&wav);
    let pcm = capture(player.id(), Duration::from_secs(5));
    let _ = player.kill();
    let tone = loudest_tone(&pcm, 440.0);
    println!("440 Hz amplitude (excluding the player): {tone:.0}");
    assert!(
        tone < ABSENT,
        "the excluded process leaked into the capture ({tone:.0})"
    );
}

#[test]
#[ignore = "needs an audio output device and Windows 11 / build 20348+"]
fn excluding_a_process_excludes_its_children_but_not_its_siblings() {
    // The shape of Warlay: one app, two children. The WebView2 (voices, must
    // be excluded) plays 440 Hz from a grandchild; the game (must be shared)
    // is a sibling playing 880 Hz. Excluding the "webview" must silence only
    // the first, in the very same capture.
    let voice = tone_wav(8, 440.0);
    let game = tone_wav(8, 880.0);
    let mut webview = play_under_a_wrapper(&voice);
    let mut game_player = play(&game);
    let pcm = capture(webview.id(), Duration::from_secs(5));
    let _ = game_player.kill();
    let _ = webview.kill();
    let (voice_tone, game_tone) = (loudest_tone(&pcm, 440.0), loudest_tone(&pcm, 880.0));
    println!("excluded subtree 440 Hz: {voice_tone:.0}; sibling 880 Hz: {game_tone:.0}");
    assert!(
        game_tone > AUDIBLE,
        "the sibling (the game) must be shared ({game_tone:.0})"
    );
    assert!(
        voice_tone < ABSENT,
        "the excluded subtree (the voices) must not be ({voice_tone:.0})"
    );
}

#[test]
#[ignore = "needs an audio output device and Windows 11 / build 20348+"]
fn the_room_has_no_pure_440_hz_without_a_player() {
    // Baseline for the thresholds above: whatever else the machine plays.
    let mut other = bystander();
    let pcm = capture(other.id(), Duration::from_secs(3));
    let _ = other.kill();
    let tone = loudest_tone(&pcm, 440.0);
    println!("baseline 440 Hz amplitude with nothing of ours playing: {tone:.0}");
    assert!(
        tone < ABSENT,
        "a 440 Hz tone was already present before we played anything"
    );
}

#[test]
#[ignore = "needs an audio output device and Windows 11 / build 20348+"]
fn the_capture_stops_when_the_handle_is_dropped() {
    let calls = Arc::new(Mutex::new(0usize));
    let counter = calls.clone();
    let handle = wasapi::start(std::process::id(), move |_| *counter.lock().unwrap() += 1).expect("start");
    std::thread::sleep(Duration::from_millis(300));
    drop(handle); // joins the capture thread
    let after_stop = *calls.lock().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        *calls.lock().unwrap(),
        after_stop,
        "frames kept arriving after the stop"
    );
}

/// Against a REAL running Warlay (see .claude/scripts/webview_real.py): that
/// app's WebView2 plays 440 Hz (standing for the other players' voices) and a
/// sibling process plays 880 Hz (standing for the game). Reads the app's PID
/// from `WARLAY_E2E_PID`.
#[test]
#[ignore = "driven by .claude/scripts/webview_real.py with a real Warlay running"]
fn the_real_webview_is_excluded_and_a_sibling_game_is_shared() {
    let app: u32 = std::env::var("WARLAY_E2E_PID")
        .expect("WARLAY_E2E_PID")
        .parse()
        .expect("numeric pid");
    let webview = super::process::child_process_named(app, "msedgewebview2.exe")
        .expect("the app has no msedgewebview2.exe child: the exclusion would fall back to the whole app");
    println!("excluding the WebView2 browser process {webview} (child of {app})");
    let pcm = capture(webview, Duration::from_secs(4));
    let (voice, game) = (loudest_tone(&pcm, 440.0), loudest_tone(&pcm, 880.0));
    println!("REAL WEBVIEW: 440 Hz (in the webview) {voice:.0}; 880 Hz (sibling) {game:.0}");
    assert!(game > AUDIBLE, "the sibling 'game' must be shared ({game:.0})");
    assert!(
        voice < ABSENT,
        "the real WebView2 audio leaked into the shared stream ({voice:.0})"
    );
}
