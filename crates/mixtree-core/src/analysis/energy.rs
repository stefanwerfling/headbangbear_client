//! Energy-indicator estimation (spec §7).
//!
//! Combines loudness (RMS), brightness (spectral centroid) and rhythmic activity
//! (spectral-flux density) into a `0..=100` indicator. **This is an estimate, not
//! ground truth** — loudness alone is never equated with musical energy — so the
//! result is always flagged `estimated: true`.

use crate::analysis::dsp::{bin_frequency, magnitude_frames, rms};
use crate::analysis::{EnergyAnalyzer, EnergyResult};

const FRAME_SIZE: usize = 1024;
const HOP: usize = 512;

/// Weighted spectral/temporal energy estimator.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpectralEnergyAnalyzer;

/// Perceptual loudness in `0..=1` from RMS (≈ -60 dB floor).
fn loudness(samples: &[f32]) -> f32 {
    let r = rms(samples);
    let db = 20.0 * (r.max(1e-6)).log10();
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

impl EnergyAnalyzer for SpectralEnergyAnalyzer {
    fn analyze(&self, samples: &[f32], sample_rate: u32) -> Option<EnergyResult> {
        if sample_rate == 0 || samples.len() < FRAME_SIZE {
            return None;
        }
        let frames = magnitude_frames(samples, FRAME_SIZE, HOP);
        if frames.is_empty() {
            return None;
        }

        // Average spectral centroid (brightness).
        let mut centroid_sum = 0.0f64;
        let mut centroid_n = 0u64;
        for frame in &frames {
            let mut num = 0.0f64;
            let mut den = 0.0f64;
            for (bin, &mag) in frame.iter().enumerate() {
                let f = bin_frequency(bin, FRAME_SIZE, sample_rate) as f64;
                num += f * mag as f64;
                den += mag as f64;
            }
            if den > 0.0 {
                centroid_sum += num / den;
                centroid_n += 1;
            }
        }
        let centroid = if centroid_n > 0 {
            centroid_sum / centroid_n as f64
        } else {
            0.0
        };

        // Mean positive spectral flux (rhythmic activity).
        let mut flux_sum = 0.0f64;
        for i in 1..frames.len() {
            let mut flux = 0.0f64;
            for (a, b) in frames[i].iter().zip(frames[i - 1].iter()) {
                let d = (a - b) as f64;
                if d > 0.0 {
                    flux += d;
                }
            }
            flux_sum += flux;
        }
        let flux_mean = flux_sum / frames.len().max(1) as f64;

        let loud = loudness(samples) as f64;
        let centroid_norm = (centroid / 4000.0).clamp(0.0, 1.0);
        let flux_norm = (flux_mean / 20.0).clamp(0.0, 1.0);

        // Loudness dominates, modulated by brightness and rhythmic activity.
        let energy = (0.5 * loud + 0.3 * flux_norm + 0.2 * centroid_norm) * 100.0;
        let energy = energy.round().clamp(0.0, 100.0) as u8;

        // Confidence is modest: this is a heuristic indicator by design.
        Some(EnergyResult {
            energy,
            confidence: 0.5,
            estimated: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn noise(amp: f32, n: usize) -> Vec<f32> {
        // Deterministic pseudo-noise (no RNG): broadband, high flux.
        let mut s = 12345u32;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1103515245).wrapping_add(12345);
                ((s >> 16) as f32 / 32768.0 - 1.0) * amp
            })
            .collect()
    }

    fn sine(freq: f32, amp: f32, sr: u32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f32 / sr as f32).sin() * amp)
            .collect()
    }

    #[test]
    fn loud_noise_beats_quiet_tone() {
        let sr = 11_025;
        let n = sr as usize * 2;
        let loud = SpectralEnergyAnalyzer.analyze(&noise(0.9, n), sr).unwrap();
        let quiet = SpectralEnergyAnalyzer
            .analyze(&sine(220.0, 0.05, sr, n), sr)
            .unwrap();
        assert!(
            loud.energy > quiet.energy,
            "loud {} vs quiet {}",
            loud.energy,
            quiet.energy
        );
        assert!(loud.estimated && quiet.estimated);
        assert!(loud.energy <= 100);
    }

    #[test]
    fn result_is_always_flagged_estimate() {
        let sr = 11_025;
        let r = SpectralEnergyAnalyzer
            .analyze(&noise(0.5, sr as usize), sr)
            .unwrap();
        assert!(r.estimated);
        assert!((0..=100).contains(&r.energy));
    }

    #[test]
    fn tiny_input_returns_none() {
        assert!(SpectralEnergyAnalyzer.analyze(&[0.0; 10], 11_025).is_none());
    }
}
