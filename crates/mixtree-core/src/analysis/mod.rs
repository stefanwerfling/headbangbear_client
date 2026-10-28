//! Analysis pipeline **interfaces** for Phase 2 (spec §7).
//!
//! Phase 1 ships the *contracts* — traits, result types and a persisted job
//! queue — so the audio-feature work in Phase 2 can slot in without touching
//! the rest of the system. Results always carry a `confidence` and an
//! `estimated` flag so the UI can honestly show uncertainty (spec §6, §7):
//! loudness is never silently equated with musical energy.
//!
//! No real DSP lives here yet. [`NullAnalyzer`] is a placeholder that reports
//! "not analyzed"; it exists so the queue can be exercised end-to-end.

pub mod bpm;
pub mod decode;
pub mod dsp;
pub mod energy;
pub mod job_queue;
pub mod key_detection;
pub mod worker;

use crate::camelot::CamelotKey;
use serde::{Deserialize, Serialize};

/// What a given analysis job computes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisKind {
    /// Tempo + beat grid.
    Bpm,
    /// Musical key.
    Key,
    /// Energy indicator.
    Energy,
    /// Song structure (intro/build/drop/breakdown/outro).
    Structure,
}

impl AnalysisKind {
    /// Stable string form stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            AnalysisKind::Bpm => "bpm",
            AnalysisKind::Key => "key",
            AnalysisKind::Energy => "energy",
            AnalysisKind::Structure => "structure",
        }
    }

    /// Parses the stored string form.
    ///
    /// Intentionally inherent (not `std::str::FromStr`): it returns `Option`,
    /// not `Result`, matching how stored enum strings are decoded elsewhere.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "bpm" => Some(AnalysisKind::Bpm),
            "key" => Some(AnalysisKind::Key),
            "energy" => Some(AnalysisKind::Energy),
            "structure" => Some(AnalysisKind::Structure),
            _ => None,
        }
    }
}

/// Result of BPM detection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BpmResult {
    /// Detected tempo in BPM.
    pub bpm: f64,
    /// Half-/double-time candidates the UI may offer for manual correction.
    pub alternates: Vec<f64>,
    /// Detection confidence, `0.0..=1.0`.
    pub confidence: f64,
    /// Beat positions in milliseconds (may be empty if only tempo is known).
    pub beat_positions_ms: Vec<u64>,
}

/// Result of key detection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyResult {
    /// Detected key on the Camelot wheel.
    pub key: CamelotKey,
    /// Detection confidence, `0.0..=1.0`.
    pub confidence: f64,
}

/// Result of energy analysis. Always an **estimate** (spec §7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyResult {
    /// Energy indicator, `0..=100`.
    pub energy: u8,
    /// Estimate confidence, `0.0..=1.0`.
    pub confidence: f64,
    /// Always `true`: energy is an estimated indicator, not ground truth.
    pub estimated: bool,
}

/// Detects tempo and beat grid from decoded PCM samples (Phase 2).
pub trait BpmDetector {
    /// Analyzes mono/interleaved samples at `sample_rate`.
    fn detect(&self, samples: &[f32], sample_rate: u32) -> Option<BpmResult>;
}

/// Estimates musical key from decoded PCM samples (Phase 2).
pub trait KeyDetector {
    /// Analyzes samples and returns a key estimate.
    fn detect(&self, samples: &[f32], sample_rate: u32) -> Option<KeyResult>;
}

/// Estimates an energy indicator from decoded PCM samples (Phase 2).
pub trait EnergyAnalyzer {
    /// Analyzes samples and returns an energy estimate.
    fn analyze(&self, samples: &[f32], sample_rate: u32) -> Option<EnergyResult>;
}

/// Placeholder analyzer used until the real Phase-2 DSP exists.
///
/// It performs no signal processing and returns `None`, signalling "not yet
/// analyzed". Wiring it through the queue proves the plumbing end-to-end.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullAnalyzer;

impl BpmDetector for NullAnalyzer {
    fn detect(&self, _samples: &[f32], _sample_rate: u32) -> Option<BpmResult> {
        None
    }
}
impl KeyDetector for NullAnalyzer {
    fn detect(&self, _samples: &[f32], _sample_rate: u32) -> Option<KeyResult> {
        None
    }
}
impl EnergyAnalyzer for NullAnalyzer {
    fn analyze(&self, _samples: &[f32], _sample_rate: u32) -> Option<EnergyResult> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_kind_roundtrips() {
        for k in [
            AnalysisKind::Bpm,
            AnalysisKind::Key,
            AnalysisKind::Energy,
            AnalysisKind::Structure,
        ] {
            assert_eq!(AnalysisKind::from_str(k.as_str()), Some(k));
        }
        assert_eq!(AnalysisKind::from_str("nope"), None);
    }

    #[test]
    fn null_analyzer_reports_not_analyzed() {
        let a = NullAnalyzer;
        assert!(BpmDetector::detect(&a, &[], 44100).is_none());
        assert!(KeyDetector::detect(&a, &[], 44100).is_none());
        assert!(a.analyze(&[], 44100).is_none());
    }
}
