//! Shared DSP primitives for the analysis detectors (Phase 2).
//!
//! Small, dependency-light helpers: windowing, downsampling, RMS and a
//! short-time magnitude spectrogram built on `rustfft`. Kept separate so BPM,
//! key and energy detection share one correct, tested implementation.

use rustfft::{num_complex::Complex, FftPlanner};

/// A periodic Hann window of length `n`.
pub fn hann(n: usize) -> Vec<f32> {
    if n <= 1 {
        return vec![1.0; n];
    }
    (0..n)
        .map(|i| {
            let x = (std::f32::consts::PI * 2.0 * i as f32) / n as f32;
            0.5 - 0.5 * x.cos()
        })
        .collect()
}

/// Root-mean-square amplitude of a buffer (`0.0` for an empty buffer).
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&x| (x as f64) * (x as f64)).sum();
    ((sum_sq / samples.len() as f64).sqrt()) as f32
}

/// Downsamples mono `samples` from `from_rate` to `to_rate` by block-averaging.
///
/// Returns the input unchanged if `to_rate >= from_rate`. Block-averaging acts
/// as a cheap anti-aliasing low-pass, which is adequate for feature extraction.
pub fn downsample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == 0 || to_rate == 0 || to_rate >= from_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (samples.len() as f64 / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let start = (i as f64 * ratio) as usize;
        let end = (((i + 1) as f64 * ratio) as usize)
            .min(samples.len())
            .max(start + 1);
        let slice = &samples[start..end];
        out.push(slice.iter().copied().sum::<f32>() / slice.len() as f32);
    }
    out
}

/// Computes a short-time magnitude spectrogram.
///
/// Each frame of `frame_size` samples is Hann-windowed, FFT'd and reduced to its
/// magnitude spectrum (the first `frame_size / 2` bins). Frames advance by `hop`.
pub fn magnitude_frames(samples: &[f32], frame_size: usize, hop: usize) -> Vec<Vec<f32>> {
    if samples.len() < frame_size || frame_size == 0 || hop == 0 {
        return Vec::new();
    }
    let window = hann(frame_size);
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(frame_size);
    let half = frame_size / 2;

    let mut frames = Vec::new();
    let mut buf = vec![Complex::new(0.0f32, 0.0); frame_size];
    let mut pos = 0;
    while pos + frame_size <= samples.len() {
        for i in 0..frame_size {
            buf[i] = Complex::new(samples[pos + i] * window[i], 0.0);
        }
        fft.process(&mut buf);
        frames.push(buf[..half].iter().map(|c| c.norm()).collect());
        pos += hop;
    }
    frames
}

/// The frequency (Hz) of FFT bin `bin` for the given `frame_size`/`sample_rate`.
pub fn bin_frequency(bin: usize, frame_size: usize, sample_rate: u32) -> f32 {
    bin as f32 * sample_rate as f32 / frame_size as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_endpoints_are_zero_ish() {
        let w = hann(8);
        assert!(w[0].abs() < 1e-6);
        assert!(w[4] > 0.9); // near the center
    }

    #[test]
    fn rms_of_constant_is_the_constant() {
        assert!((rms(&[0.5, 0.5, 0.5, 0.5]) - 0.5).abs() < 1e-6);
        assert_eq!(rms(&[]), 0.0);
    }

    #[test]
    fn downsample_halves_length() {
        let s: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        let d = downsample(&s, 44100, 22050);
        assert!((d.len() as i64 - 500).abs() <= 1);
    }

    #[test]
    fn downsample_noop_when_target_higher() {
        let s = vec![1.0, 2.0, 3.0];
        assert_eq!(downsample(&s, 100, 200), s);
    }

    #[test]
    fn fft_peaks_at_input_sine_frequency() {
        // 1000 Hz sine at 16 kHz, frame 1024 → peak bin ≈ 1000/(16000/1024) ≈ 64.
        let sr = 16_000u32;
        let freq = 1000.0f32;
        let n = 4096;
        let sig: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sr as f32).sin())
            .collect();
        let frames = magnitude_frames(&sig, 1024, 512);
        assert!(!frames.is_empty());
        let spectrum = &frames[0];
        let peak_bin = spectrum
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        let peak_freq = bin_frequency(peak_bin, 1024, sr);
        assert!(
            (peak_freq - freq).abs() < 40.0,
            "peak at {peak_freq} Hz, expected ~{freq}"
        );
    }
}
