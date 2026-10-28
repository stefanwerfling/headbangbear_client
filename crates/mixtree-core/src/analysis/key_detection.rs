//! Musical key detection (spec §7): chroma extraction + Krumhansl-Schmuckler
//! key-profile correlation, mapped to the Camelot wheel.

use crate::analysis::dsp::{bin_frequency, magnitude_frames};
use crate::analysis::{KeyDetector, KeyResult};
use crate::camelot::CamelotKey;

const FRAME_SIZE: usize = 4096;
const HOP: usize = 2048;
const MIN_FREQ: f32 = 55.0; // A1
const MAX_FREQ: f32 = 5000.0;

/// Krumhansl-Schmuckler major key profile (pitch-class weights, tonic at index 0).
const MAJOR_PROFILE: [f64; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
/// Krumhansl-Schmuckler minor key profile.
const MINOR_PROFILE: [f64; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

// Camelot wheel number for each tonic pitch class (0 = C .. 11 = B).
const MAJOR_CAMELOT: [u8; 12] = [8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6, 1];
const MINOR_CAMELOT: [u8; 12] = [5, 12, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10];

/// Chroma + key-profile correlation detector.
#[derive(Debug, Default, Clone, Copy)]
pub struct ChromaKeyDetector;

/// Computes a normalized 12-bin chroma vector (index 0 = C) for the whole buffer.
pub fn chroma(samples: &[f32], sample_rate: u32) -> [f32; 12] {
    let mut chroma = [0.0f32; 12];
    if sample_rate == 0 {
        return chroma;
    }
    let frames = magnitude_frames(samples, FRAME_SIZE, HOP);
    for frame in &frames {
        for (bin, &mag) in frame.iter().enumerate() {
            let freq = bin_frequency(bin, FRAME_SIZE, sample_rate);
            if !(MIN_FREQ..=MAX_FREQ).contains(&freq) {
                continue;
            }
            // MIDI pitch → pitch class (0 = C).
            let midi = 69.0 + 12.0 * (freq / 440.0).log2();
            let pc = (midi.round() as i32).rem_euclid(12) as usize;
            chroma[pc] += mag;
        }
    }
    let max = chroma.iter().copied().fold(0.0f32, f32::max);
    if max > 0.0 {
        for v in &mut chroma {
            *v /= max;
        }
    }
    chroma
}

/// Pearson correlation between a chroma vector and a profile rotated to `tonic`.
fn correlation(chroma: &[f32; 12], profile: &[f64; 12], tonic: usize) -> f64 {
    let rotated: Vec<f64> = (0..12).map(|i| profile[(i + 12 - tonic) % 12]).collect();
    let cm = chroma.iter().map(|&x| x as f64).sum::<f64>() / 12.0;
    let pm = rotated.iter().sum::<f64>() / 12.0;
    let mut num = 0.0;
    let mut dc = 0.0;
    let mut dp = 0.0;
    for i in 0..12 {
        let a = chroma[i] as f64 - cm;
        let b = rotated[i] - pm;
        num += a * b;
        dc += a * a;
        dp += b * b;
    }
    if dc <= 0.0 || dp <= 0.0 {
        return 0.0;
    }
    num / (dc.sqrt() * dp.sqrt())
}

impl KeyDetector for ChromaKeyDetector {
    fn detect(&self, samples: &[f32], sample_rate: u32) -> Option<KeyResult> {
        let chroma = chroma(samples, sample_rate);
        if chroma.iter().all(|&x| x == 0.0) {
            return None;
        }
        // Score all 24 keys, keep the best and runner-up for a confidence margin.
        let mut scored: Vec<(f64, CamelotKey)> = Vec::with_capacity(24);
        for tonic in 0..12 {
            let maj = correlation(&chroma, &MAJOR_PROFILE, tonic);
            scored.push((maj, CamelotKey::new(MAJOR_CAMELOT[tonic], true).unwrap()));
            let min = correlation(&chroma, &MINOR_PROFILE, tonic);
            scored.push((min, CamelotKey::new(MINOR_CAMELOT[tonic], false).unwrap()));
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let (best_score, key) = scored[0];
        let second = scored.get(1).map(|s| s.0).unwrap_or(0.0);
        // Confidence from the margin between the best and runner-up correlations.
        let confidence = ((best_score - second).max(0.0) * 3.0).clamp(0.0, 1.0);

        Some(KeyResult { key, confidence })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn sine(freq: f32, sr: u32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f32 / sr as f32).sin())
            .collect()
    }

    fn mix(freqs: &[f32], sr: u32, n: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; n];
        for &f in freqs {
            for (i, s) in sine(f, sr, n).into_iter().enumerate() {
                out[i] += s / freqs.len() as f32;
            }
        }
        out
    }

    #[test]
    fn chroma_peaks_at_input_pitch_class() {
        // A4 = 440 Hz → pitch class A (index 9).
        let sr = 11_025;
        let sig = sine(440.0, sr, sr as usize * 2);
        let c = chroma(&sig, sr);
        let peak = c
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        assert_eq!(peak, 9, "chroma peak should be A, got pc {peak}");
    }

    #[test]
    fn detects_c_major_or_relative_a_minor() {
        // C major triad: C4, E4, G4.
        let sr = 11_025;
        let sig = mix(&[261.63, 329.63, 392.0], sr, sr as usize * 3);
        let r = ChromaKeyDetector.detect(&sig, sr).expect("key detected");
        let s = r.key.to_string();
        // C major is 8B; its relative minor A is 8A — either is musically correct.
        assert!(s == "8B" || s == "8A", "expected 8B/8A, got {s}");
    }

    #[test]
    fn silence_returns_none() {
        assert!(ChromaKeyDetector.detect(&vec![0.0; 8192], 11_025).is_none());
    }

    #[test]
    fn camelot_tables_are_consistent() {
        // A minor must map to 8A and C major to 8B (sanity of the tables).
        assert_eq!(MINOR_CAMELOT[9], 8); // A
        assert_eq!(MAJOR_CAMELOT[0], 8); // C
    }
}
