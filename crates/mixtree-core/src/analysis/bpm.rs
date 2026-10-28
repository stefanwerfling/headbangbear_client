//! Tempo (BPM) and beat-grid detection (spec §7).
//!
//! Pipeline: spectral-flux onset envelope → autocorrelation over the tempo range
//! → parabolic peak interpolation for sub-lag precision → comb-filter phase for
//! the beat grid. Half-/double-time candidates are always reported so the UI can
//! offer manual correction.

use crate::analysis::dsp::magnitude_frames;
use crate::analysis::{BpmDetector, BpmResult};

const FRAME_SIZE: usize = 1024;
const HOP: usize = 128;
const MIN_BPM: f64 = 60.0;
const MAX_BPM: f64 = 200.0;

/// Autocorrelation-based tempo detector.
#[derive(Debug, Default, Clone, Copy)]
pub struct AutocorrelationBpmDetector;

/// Half-wave-rectified spectral flux onset envelope (one value per frame).
fn onset_envelope(samples: &[f32]) -> Vec<f32> {
    let frames = magnitude_frames(samples, FRAME_SIZE, HOP);
    if frames.len() < 2 {
        return Vec::new();
    }
    let mut env = Vec::with_capacity(frames.len() - 1);
    for i in 1..frames.len() {
        let mut flux = 0.0f32;
        for (a, b) in frames[i].iter().zip(frames[i - 1].iter()) {
            let d = a - b;
            if d > 0.0 {
                flux += d;
            }
        }
        env.push(flux);
    }
    // Mean-remove and half-wave rectify to emphasize peaks.
    let mean = env.iter().copied().sum::<f32>() / env.len() as f32;
    for v in &mut env {
        *v = (*v - mean).max(0.0);
    }
    env
}

/// Normalized autocorrelation of `env` at integer `lag`.
fn autocorr(env: &[f32], lag: usize) -> f64 {
    if lag == 0 || lag >= env.len() {
        return 0.0;
    }
    let mut sum = 0.0f64;
    for i in 0..env.len() - lag {
        sum += env[i] as f64 * env[i + lag] as f64;
    }
    sum / (env.len() - lag) as f64
}

impl BpmDetector for AutocorrelationBpmDetector {
    fn detect(&self, samples: &[f32], sample_rate: u32) -> Option<BpmResult> {
        if sample_rate == 0 {
            return None;
        }
        let env = onset_envelope(samples);
        if env.len() < 64 {
            return None;
        }
        let fps = sample_rate as f64 / HOP as f64;
        let min_lag = (60.0 * fps / MAX_BPM).floor().max(1.0) as usize;
        let max_lag = (60.0 * fps / MIN_BPM).ceil() as usize;
        if max_lag >= env.len() {
            return None;
        }

        // Find the lag with the strongest autocorrelation in the tempo range.
        let mut best_lag = min_lag;
        let mut best_val = f64::MIN;
        let mut sum_val = 0.0;
        let mut count = 0;
        for lag in min_lag..=max_lag {
            let v = autocorr(&env, lag);
            sum_val += v;
            count += 1;
            if v > best_val {
                best_val = v;
                best_lag = lag;
            }
        }
        let mean_val = sum_val / count.max(1) as f64;

        // Parabolic interpolation around the integer peak for sub-lag precision.
        let refined_lag = parabolic_peak(&env, best_lag);
        let bpm = 60.0 * fps / refined_lag;

        // Confidence: how much the peak stands out from the average.
        let confidence = if mean_val > 0.0 {
            (((best_val / mean_val) - 1.0) / 2.0).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let beat_positions_ms = beat_grid(&env, best_lag, fps);

        let mut alternates = vec![bpm / 2.0, bpm * 2.0];
        alternates.retain(|b| (MIN_BPM..=MAX_BPM).contains(b));

        Some(BpmResult {
            bpm: (bpm * 10.0).round() / 10.0,
            alternates: alternates
                .into_iter()
                .map(|b| (b * 10.0).round() / 10.0)
                .collect(),
            confidence,
            beat_positions_ms,
        })
    }
}

/// Parabolic interpolation of the autocorrelation peak at `lag`.
fn parabolic_peak(env: &[f32], lag: usize) -> f64 {
    if lag == 0 || lag + 1 >= env.len() {
        return lag as f64;
    }
    let ym1 = autocorr(env, lag - 1);
    let y0 = autocorr(env, lag);
    let yp1 = autocorr(env, lag + 1);
    let denom = ym1 - 2.0 * y0 + yp1;
    if denom.abs() < 1e-12 {
        return lag as f64;
    }
    let delta = 0.5 * (ym1 - yp1) / denom;
    lag as f64 + delta.clamp(-0.5, 0.5)
}

/// Comb-filter phase estimate → absolute beat times in milliseconds.
fn beat_grid(env: &[f32], lag: usize, fps: f64) -> Vec<u64> {
    if lag == 0 {
        return Vec::new();
    }
    // Find the start offset within one beat period maximizing the pulse train.
    let mut best_offset = 0usize;
    let mut best_sum = f64::MIN;
    for offset in 0..lag {
        let mut sum = 0.0f64;
        let mut k = offset;
        while k < env.len() {
            sum += env[k] as f64;
            k += lag;
        }
        if sum > best_sum {
            best_sum = sum;
            best_offset = offset;
        }
    }
    let frame_ms = 1000.0 / fps;
    let mut beats = Vec::new();
    let mut k = best_offset;
    while k < env.len() {
        beats.push((k as f64 * frame_ms).round() as u64);
        k += lag;
    }
    beats
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A click train: short impulses every `interval` seconds.
    fn click_train(bpm: f64, sample_rate: u32, seconds: f64) -> Vec<f32> {
        let n = (sample_rate as f64 * seconds) as usize;
        let mut sig = vec![0.0f32; n];
        let period = (60.0 / bpm * sample_rate as f64) as usize;
        let mut i = 0;
        while i < n {
            // A short decaying click so it has broadband onset content.
            for j in 0..64 {
                if i + j < n {
                    sig[i + j] += (1.0 - j as f32 / 64.0) * ((j as f32) * 0.5).sin();
                }
            }
            i += period;
        }
        sig
    }

    fn detected(bpm: f64) -> BpmResult {
        let sr = 22_050;
        let sig = click_train(bpm, sr, 12.0);
        AutocorrelationBpmDetector
            .detect(&sig, sr)
            .expect("bpm detected")
    }

    fn close(result: &BpmResult, target: f64, tol: f64) -> bool {
        let mut cands = vec![result.bpm];
        cands.extend(result.alternates.iter().copied());
        cands.iter().any(|&b| (b - target).abs() <= tol)
    }

    #[test]
    fn detects_120_bpm() {
        let r = detected(120.0);
        assert!(close(&r, 120.0, 3.0), "got {:?}", r);
    }

    #[test]
    fn detects_128_bpm() {
        let r = detected(128.0);
        assert!(close(&r, 128.0, 3.0), "got {:?}", r);
    }

    #[test]
    fn detects_140_bpm() {
        let r = detected(140.0);
        assert!(close(&r, 140.0, 3.5), "got {:?}", r);
    }

    #[test]
    fn reports_beat_grid_and_confidence() {
        let r = detected(120.0);
        assert!(!r.beat_positions_ms.is_empty());
        assert!(r.confidence > 0.0);
        // Beat spacing should be ~500 ms for 120 BPM.
        if r.beat_positions_ms.len() >= 3 {
            let d = r.beat_positions_ms[2] - r.beat_positions_ms[1];
            assert!((d as f64 - 500.0).abs() < 60.0, "beat spacing {d} ms");
        }
    }

    #[test]
    fn silence_or_tiny_input_returns_none() {
        assert!(AutocorrelationBpmDetector.detect(&[], 22_050).is_none());
        assert!(AutocorrelationBpmDetector
            .detect(&vec![0.0; 100], 22_050)
            .is_none());
    }
}
