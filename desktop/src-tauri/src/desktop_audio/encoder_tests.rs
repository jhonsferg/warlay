use super::*;
use audiopus::coder::Decoder;

/// `frames` stereo frames of a sine at `hz`, peak `amp`.
fn sine(frames: usize, hz: f32, amp: f32) -> Vec<i16> {
    (0..frames)
        .flat_map(|i| {
            let s = (2.0 * std::f32::consts::PI * hz * i as f32 / SAMPLE_RATE as f32).sin();
            let v = (s * amp * i16::MAX as f32) as i16;
            [v, v]
        })
        .collect()
}

fn rms(pcm: &[i16]) -> f64 {
    (pcm.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / pcm.len().max(1) as f64).sqrt()
}

fn decode_all(packets: &[Vec<u8>]) -> Vec<i16> {
    let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo).unwrap();
    let mut out = Vec::new();
    for p in packets {
        let mut frame = vec![0i16; FRAME_SAMPLES];
        let n = decoder.decode(Some(p.as_slice()), &mut frame[..], false).unwrap();
        out.extend_from_slice(&frame[..n * CHANNELS]);
    }
    out
}

#[test]
fn one_second_of_audio_becomes_fifty_packets() {
    let mut enc = DesktopEncoder::new().unwrap();
    let mut packets = Vec::new();
    let n = enc.push(&sine(48_000, 440.0, 0.5), |p| packets.push(p.to_vec()));
    assert_eq!(n, 50);
    assert_eq!(packets.len(), 50);
    assert_eq!(enc.buffered(), 0);
    // 64 kbit/s is ~160 bytes per 20 ms: far below a datagram's MTU.
    assert!(
        packets.iter().all(|p| !p.is_empty() && p.len() < 600),
        "{:?}",
        packets.iter().map(Vec::len).max()
    );
}

#[test]
fn chunk_sizes_do_not_change_what_comes_out() {
    // WASAPI delivers odd-sized packets: the result must not depend on them.
    let pcm = sine(24_000, 330.0, 0.4);
    let encode = |chunk: usize| {
        let mut enc = DesktopEncoder::new().unwrap();
        let mut out = Vec::new();
        for c in pcm.chunks(chunk) {
            enc.push(c, |p| out.push(p.to_vec()));
        }
        (out, enc.buffered())
    };
    let (whole, _) = encode(pcm.len());
    let (ragged, _) = encode(1234);
    let (tiny, _) = encode(2);
    assert_eq!(whole.len(), 25);
    assert_eq!(whole, ragged);
    assert_eq!(whole, tiny);
}

#[test]
fn a_partial_frame_waits_for_the_rest() {
    let mut enc = DesktopEncoder::new().unwrap();
    let half = sine(480, 440.0, 0.5); // 10 ms
    assert_eq!(enc.push(&half, |_| panic!("must not emit yet")), 0);
    assert_eq!(enc.buffered(), half.len());
    assert_eq!(enc.push(&half, |_| {}), 1);
    assert_eq!(enc.buffered(), 0);
}

#[test]
fn digital_silence_costs_no_bandwidth() {
    let mut enc = DesktopEncoder::new().unwrap();
    assert_eq!(
        enc.push(&vec![0i16; FRAME_SAMPLES * 10], |_| panic!("silence must not be sent")),
        0
    );
    // ...but a single audible sample in a frame is enough to send it.
    let mut frame = vec![0i16; FRAME_SAMPLES];
    frame[100] = 3000;
    assert_eq!(enc.push(&frame, |_| {}), 1);
}

#[test]
fn the_audio_survives_a_round_trip_through_opus() {
    let input = sine(48_000, 440.0, 0.5);
    let mut enc = DesktopEncoder::new().unwrap();
    let mut packets = Vec::new();
    enc.push(&input, |p| packets.push(p.to_vec()));
    let decoded = decode_all(&packets);

    assert_eq!(decoded.len(), input.len());
    // Skip the codec's start-up frames, then the level must match closely.
    let skip = FRAME_SAMPLES * 5;
    let (a, b) = (rms(&input[skip..]), rms(&decoded[skip..]));
    let db = 20.0 * (b / a).log10();
    assert!(db.abs() < 1.5, "level changed by {db:.2} dB");
}
