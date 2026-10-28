//! Audio decoding to mono PCM via `symphonia` (pure Rust, no system deps).
//!
//! The analysis detectors work on a mono `f32` buffer at a reduced sample rate
//! (default 11 025 Hz) which is ample for tempo/key/energy features and keeps
//! analysis fast on large libraries.

use crate::analysis::dsp;
use crate::error::{CoreError, Result};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Default sample rate the analysis pipeline downsamples to.
pub const ANALYSIS_SAMPLE_RATE: u32 = 11_025;

/// Decoded mono audio.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    /// Mono samples in `-1.0..=1.0`.
    pub samples: Vec<f32>,
    /// Sample rate of `samples`.
    pub sample_rate: u32,
}

fn decode_err(path: &Path, e: impl std::fmt::Display) -> CoreError {
    CoreError::Metadata {
        path: path.display().to_string(),
        message: format!("decode: {e}"),
    }
}

/// Decodes `path` into mono `f32` samples at the file's native sample rate.
pub fn decode_to_mono(path: &Path) -> Result<DecodedAudio> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| decode_err(path, e))?;
    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| decode_err(path, "no default audio track"))?;
    let track_id = track.id;
    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(44_100);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| decode_err(path, e))?;

    let mut samples: Vec<f32> = Vec::new();
    let mut sbuf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            // End of stream / IO end: stop cleanly with what we have.
            Err(SymphoniaError::IoError(_)) => break,
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(decode_err(path, e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                sample_rate = spec.rate;
                let channels = spec.channels.count().max(1);
                if sbuf.is_none() {
                    sbuf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
                }
                let buf = sbuf.as_mut().unwrap();
                buf.copy_interleaved_ref(decoded);
                for frame in buf.samples().chunks(channels) {
                    let mono = frame.iter().copied().sum::<f32>() / channels as f32;
                    samples.push(mono);
                }
            }
            // A single corrupt packet should not abort the whole decode.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(_)) => break,
            Err(e) => return Err(decode_err(path, e)),
        }
    }

    if samples.is_empty() {
        return Err(decode_err(path, "no audio samples decoded"));
    }
    Ok(DecodedAudio {
        samples,
        sample_rate,
    })
}

/// Interleaved audio for playback (up to 2 channels, native sample rate).
#[derive(Debug, Clone)]
pub struct InterleavedAudio {
    /// Interleaved samples (`channels` per frame).
    pub samples: Vec<f32>,
    /// Channel count (1 or 2).
    pub channels: u16,
    /// Native sample rate.
    pub sample_rate: u32,
}

/// Decodes `path` into interleaved samples (mono or stereo) at the native rate,
/// for playback by the audio engine. Sources with >2 channels are downmixed to
/// stereo by taking the first two channels.
pub fn decode_interleaved(path: &Path) -> Result<InterleavedAudio> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| decode_err(path, e))?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| decode_err(path, "no default audio track"))?;
    let track_id = track.id;
    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(44_100);
    let mut out_channels: u16 = 2;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| decode_err(path, e))?;

    let mut samples: Vec<f32> = Vec::new();
    let mut sbuf: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(_)) | Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(decode_err(path, e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                sample_rate = spec.rate;
                let ch = spec.channels.count().max(1);
                out_channels = ch.min(2) as u16;
                if sbuf.is_none() {
                    sbuf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
                }
                let buf = sbuf.as_mut().unwrap();
                buf.copy_interleaved_ref(decoded);
                for frame in buf.samples().chunks(ch) {
                    if ch == 1 {
                        samples.push(frame[0]);
                    } else {
                        samples.push(frame[0]);
                        samples.push(frame[1]);
                    }
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(_)) => break,
            Err(e) => return Err(decode_err(path, e)),
        }
    }
    if samples.is_empty() {
        return Err(decode_err(path, "no audio samples decoded"));
    }
    Ok(InterleavedAudio {
        samples,
        channels: out_channels,
        sample_rate,
    })
}

/// Decodes `path` and downsamples to [`ANALYSIS_SAMPLE_RATE`] mono for analysis.
pub fn decode_for_analysis(path: &Path) -> Result<DecodedAudio> {
    let full = decode_to_mono(path)?;
    let samples = dsp::downsample(&full.samples, full.sample_rate, ANALYSIS_SAMPLE_RATE);
    Ok(DecodedAudio {
        samples,
        sample_rate: ANALYSIS_SAMPLE_RATE.min(full.sample_rate),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Writes a minimal 16-bit PCM mono WAV file for decode tests.
    fn write_wav(path: &Path, samples: &[i16], sample_rate: u32) {
        let mut f = std::fs::File::create(path).unwrap();
        let data_len = (samples.len() * 2) as u32;
        let byte_rate = sample_rate * 2;
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        f.write_all(b"WAVE").unwrap();
        f.write_all(b"fmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap(); // fmt chunk size
        f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
        f.write_all(&1u16.to_le_bytes()).unwrap(); // mono
        f.write_all(&sample_rate.to_le_bytes()).unwrap();
        f.write_all(&byte_rate.to_le_bytes()).unwrap();
        f.write_all(&2u16.to_le_bytes()).unwrap(); // block align
        f.write_all(&16u16.to_le_bytes()).unwrap(); // bits per sample
        f.write_all(b"data").unwrap();
        f.write_all(&data_len.to_le_bytes()).unwrap();
        for &s in samples {
            f.write_all(&s.to_le_bytes()).unwrap();
        }
    }

    #[test]
    fn decodes_generated_wav() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        let sr = 22_050u32;
        let samples: Vec<i16> = (0..sr)
            .map(|i| {
                ((2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin() * 12000.0) as i16
            })
            .collect();
        write_wav(&path, &samples, sr);

        let decoded = decode_to_mono(&path).unwrap();
        assert_eq!(decoded.sample_rate, sr);
        assert!(decoded.samples.len() as i64 >= sr as i64 - 16);
        assert!(decoded.samples.iter().any(|&x| x.abs() > 0.1));
    }

    #[test]
    fn decode_for_analysis_downsamples() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        let sr = 44_100u32;
        let samples: Vec<i16> = (0..sr).map(|i| ((i % 100) as i16 - 50) * 100).collect();
        write_wav(&path, &samples, sr);
        let decoded = decode_for_analysis(&path).unwrap();
        assert_eq!(decoded.sample_rate, ANALYSIS_SAMPLE_RATE);
        assert!(decoded.samples.len() < sr as usize);
    }

    #[test]
    fn corrupt_file_errors_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.wav");
        std::fs::File::create(&path)
            .unwrap()
            .write_all(b"not a wav")
            .unwrap();
        assert!(matches!(
            decode_to_mono(&path),
            Err(CoreError::Metadata { .. })
        ));
    }
}
