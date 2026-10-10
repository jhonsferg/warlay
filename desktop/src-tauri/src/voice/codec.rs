use audiopus::coder::{Decoder, Encoder};
use audiopus::{Application, Channels, SampleRate};
use std::error::Error;

pub struct VoiceCodec;

pub const SAMPLE_RATE: u32 = 48000;
pub const FRAME_SIZE_MS: u32 = 20;
pub const SAMPLES_PER_FRAME: usize = (SAMPLE_RATE * FRAME_SIZE_MS / 1000) as usize; // 960

pub fn create_encoder() -> Result<Encoder, Box<dyn Error + Send + Sync>> {
    let encoder = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip)?;
    Ok(encoder)
}

pub fn create_decoder() -> Result<Decoder, Box<dyn Error + Send + Sync>> {
    let decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono)?;
    Ok(decoder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opus_round_trip() {
        let encoder = create_encoder().expect("create encoder");
        let mut decoder = create_decoder().expect("create decoder");

        // Generate a 440 Hz sine wave PCM frame (20ms = 960 samples)
        let mut original_pcm = vec![0i16; SAMPLES_PER_FRAME];
        for i in 0..SAMPLES_PER_FRAME {
            let t = i as f32 / SAMPLE_RATE as f32;
            let val = (t * 440.0 * 2.0 * std::f32::consts::PI).sin();
            original_pcm[i] = (val * 10000.0) as i16;
        }

        // Encode to Opus
        let mut opus_buf = vec![0u8; 512];
        let encoded_len = encoder.encode(&original_pcm, &mut opus_buf).expect("opus encode");
        let opus_data = &opus_buf[..encoded_len];
        assert!(encoded_len > 0);

        // Decode back to PCM
        let mut decoded_pcm = vec![0i16; SAMPLES_PER_FRAME];
        let decoded_samples = decoder
            .decode(Some(opus_data), &mut decoded_pcm, false)
            .expect("opus decode");

        assert_eq!(decoded_samples, SAMPLES_PER_FRAME);

        // Compare RMS power (invariant to Opus internal filter/phase delay)
        let mut orig_sum_sq = 0.0f64;
        let mut dec_sum_sq = 0.0f64;
        for i in 0..SAMPLES_PER_FRAME {
            orig_sum_sq += (original_pcm[i] as f64) * (original_pcm[i] as f64);
            dec_sum_sq += (decoded_pcm[i] as f64) * (decoded_pcm[i] as f64);
        }

        let rms_orig = (orig_sum_sq / SAMPLES_PER_FRAME as f64).sqrt();
        let rms_decoded = (dec_sum_sq / SAMPLES_PER_FRAME as f64).sqrt();

        let rms_error_diff = (rms_orig - rms_decoded).abs() / rms_orig;
        assert!(
            rms_error_diff < 0.3,
            "Opus RMS energy difference too high: {}",
            rms_error_diff
        );
    }
}
