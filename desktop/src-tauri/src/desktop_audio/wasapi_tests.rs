//! Real-audio checks of the process-loopback capture. They need an audio
//! output device and Windows 11 / 10 build 20348+, so they are `#[ignore]`d;
//! run them on purpose:
//!
//!     cargo test -p desktop wasapi -- --ignored --test-threads=1
//!
//! A tone is played by an UNRELATED process (PowerShell's SoundPlayer) and the
//! capture must hear it -- unless that very process is the one excluded.

use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::encoder::{CHANNELS, SAMPLE_RATE};
use super::wasapi;

/// Amplitude (in i16 units) of a pure tone at `hz` inside interleaved stereo
/// `pcm`, by the Goertzel algorithm on the left channel. Other audio playing
/// on the machine has no such pure 440 Hz line, so this tells OUR tone apart
/// from the background, which an overall level cannot.
fn tone_amplitude(pcm: &[i16], hz: f64) -> f64 {
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

fn rms(pcm: &[i16]) -> f64 {
    (pcm.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / pcm.len().max(1) as f64).sqrt()
}

/// A WAV of a 440 Hz tone, `seconds` long, 48 kHz stereo 16-bit.
fn tone_wav(seconds: u32) -> std::path::PathBuf {
    let frames = SAMPLE_RATE * seconds;
    let mut data = Vec::with_capacity(frames as usize * 4);
    for i in 0..frames {
        let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SAMPLE_RATE as f32).sin();
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
    let path = std::env::temp_dir().join(format!("warlay-tone-{}.wav", std::process::id()));
    std::fs::write(&path, wav).unwrap();
    path
}

fn play(wav: &std::path::Path) -> Child {
    Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("(New-Object Media.SoundPlayer '{}').PlaySync()", wav.display()),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the tone player")
}

/// A process unrelated to the player, only there to be excluded.
fn bystander() -> Child {
    Command::new("ping")
        .args(["-n", "30", "127.0.0.1"])
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn a bystander")
}

/// Captures for `window`, excluding `exclude_pid`. Returns the strongest 440 Hz
/// amplitude over any 0.5 s block (the tone starts a moment after the player does)
/// and the loudest overall 100 ms block, for the log.
fn capture_loudest(exclude_pid: u32, window: Duration) -> (f64, f64) {
    let blocks = Arc::new(Mutex::new(Vec::<i16>::new()));
    let sink = blocks.clone();
    let handle = wasapi::start(exclude_pid, move |pcm| sink.lock().unwrap().extend_from_slice(pcm))
        .expect("start the process-loopback capture");
    std::thread::sleep(window);
    drop(handle);
    let all = blocks.lock().unwrap();
    let overall = (SAMPLE_RATE as usize / 10) * CHANNELS;
    let half_second = (SAMPLE_RATE as usize / 2) * CHANNELS;
    let loudest = all.chunks(overall).map(rms).fold(0.0, f64::max);
    let tone = all.chunks(half_second).map(|c| tone_amplitude(c, 440.0)).fold(0.0, f64::max);
    (tone, loudest)
}

#[test]
#[ignore = "needs an audio output device and Windows 11 / build 20348+"]
fn audio_from_another_process_is_captured() {
    let wav = tone_wav(8);
    let mut other = bystander();
    let mut player = play(&wav);
    let (tone, loudest) = capture_loudest(other.id(), Duration::from_secs(5));
    let _ = player.kill();
    let _ = other.kill();
    println!("440 Hz amplitude (excluding an unrelated process): {tone:.0}, loudest block {loudest:.0}");
    // The tone is a 0.5-scale sine: ~16000 at the source.
    assert!(tone > 8000.0, "the tone played by another process was not heard (440 Hz amplitude {tone:.0})");
}

#[test]
#[ignore = "needs an audio output device and Windows 11 / build 20348+"]
fn the_room_is_quiet_without_a_player() {
    // Baseline: whatever else this machine plays (a browser, system sounds).
    let mut other = bystander();
    let (tone, loudest) = capture_loudest(other.id(), Duration::from_secs(3));
    let _ = other.kill();
    println!("baseline, nothing of ours playing: 440 Hz amplitude {tone:.1}, loudest block {loudest:.0}");
    assert!(tone < 1500.0, "a 440 Hz tone was already present before we played anything");
}

#[test]
#[ignore = "needs an audio output device and Windows 11 / build 20348+"]
fn audio_from_the_excluded_process_tree_is_not_captured() {
    let wav = tone_wav(8);
    let mut player = play(&wav);
    // Exclude the player itself: the very same tone must now be inaudible.
    let (tone, loudest) = capture_loudest(player.id(), Duration::from_secs(5));
    let _ = player.kill();
    println!("440 Hz amplitude (excluding the player): {tone:.0}, loudest block {loudest:.0}");
    // Background audio on the machine may be loud, but it has no 440 Hz line.
    assert!(tone < 1500.0, "the excluded process leaked into the capture (440 Hz amplitude {tone:.0})");
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
    assert_eq!(*calls.lock().unwrap(), after_stop, "frames kept arriving after the stop");
}
