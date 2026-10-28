//! Transition scoring: how well does track B follow track A?
//!
//! Implements the multi-dimensional, normalized scoring model from spec §4.
//! Every sub-score is in `0.0..=1.0`; the final [`TransitionScore::total`] is
//! the weighted sum normalized by the positive weights, so it is also in
//! `0.0..=1.0`. No arbitrary magic numbers: every component is documented.

use crate::camelot::CamelotKey;
use crate::configuration::{MixGoal, TransitionWeights};
use crate::models::{Genre, Track};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// BPM difference (in BPM) beyond which compatibility is considered zero.
const MAX_BPM_DIFF: f64 = 16.0;

/// Resolves genre relationships (parent chain + cross-genre affinity) so the
/// scorer can compute genre/style similarity without re-reading the taxonomy.
#[derive(Debug, Clone, Default)]
pub struct GenreContext {
    parent_of: HashMap<String, Option<String>>,
}

impl GenreContext {
    /// Builds a context from the configured genre taxonomy.
    pub fn from_genres(genres: &[Genre]) -> Self {
        let parent_of = genres
            .iter()
            .map(|g| (g.id.clone(), g.parent.clone()))
            .collect();
        GenreContext { parent_of }
    }

    /// Walks up to the top-level genre id for any genre/sub-genre id.
    pub fn top_level(&self, id: &str) -> String {
        let mut cur = id.to_string();
        // Bounded walk (taxonomy is shallow) guards against accidental cycles.
        for _ in 0..8 {
            match self.parent_of.get(&cur) {
                Some(Some(parent)) => cur = parent.clone(),
                _ => break,
            }
        }
        cur
    }

    /// Genre/style similarity in `0.0..=1.0` for two tracks.
    ///
    /// - identical sub-genre → `1.0`
    /// - same top-level genre → `0.8`
    /// - affine top-level genres (e.g. techno↔house) → `0.5`
    /// - otherwise → `0.3`
    /// - either genre unknown → `0.5` (neutral)
    pub fn similarity(&self, a: &Track, b: &Track) -> f64 {
        let (ag, bg) = match (a.genre.as_deref(), b.genre.as_deref()) {
            (Some(ag), Some(bg)) => (ag, bg),
            _ => return 0.5,
        };
        // Exact sub-genre match.
        if a.subgenre.is_some() && a.subgenre == b.subgenre {
            return 1.0;
        }
        let at = self.top_level(a.subgenre.as_deref().unwrap_or(ag));
        let bt = self.top_level(b.subgenre.as_deref().unwrap_or(bg));
        if at == bt {
            return 0.8;
        }
        if affine_genres(&at, &bt) {
            0.5
        } else {
            0.3
        }
    }
}

/// Symmetric affinity between top-level genres that commonly bridge well.
fn affine_genres(a: &str, b: &str) -> bool {
    const PAIRS: &[(&str, &str)] = &[
        ("techno", "house"),
        ("techno", "trance"),
        ("house", "trance"),
        ("house", "progressive-house"),
        ("trance", "ambient"),
        ("ambient", "downtempo"),
        ("techno", "hardstyle"),
        ("hardstyle", "electro"),
        ("electro", "breakbeat"),
        ("house", "electro"),
    ];
    PAIRS
        .iter()
        .any(|&(x, y)| (x == a && y == b) || (x == b && y == a))
}

/// A fully-explained transition score between two tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionScore {
    /// Overall normalized score, `0.0..=1.0`.
    pub total: f64,
    /// BPM compatibility sub-score.
    pub bpm: f64,
    /// Harmonic (key) compatibility sub-score.
    pub key: f64,
    /// Energy closeness sub-score.
    pub energy: f64,
    /// Genre/style similarity sub-score.
    pub genre: f64,
    /// Overall mixability sub-score.
    pub mixability: f64,
    /// Alignment with the active mix goal.
    pub goal: f64,
    /// Repetition penalty (raw, `0.0..=1.0`) applied to already-played tracks.
    pub repetition_penalty: f64,
    /// Signed BPM difference `to - from`, if both BPMs are known (display hint).
    pub bpm_diff: Option<f64>,
    /// Human-readable rationale lines for the UI (spec §4: explainability).
    pub notes: Vec<String>,
}

/// Effective BPM difference, accounting for half-/double-time matches.
fn effective_bpm_diff(a: f64, b: f64) -> f64 {
    let direct = (a - b).abs();
    let half = (a - 2.0 * b).abs();
    let double = (2.0 * a - b).abs();
    direct.min(half).min(double)
}

/// BPM compatibility in `0.0..=1.0`; unknown BPM yields a neutral `0.5`.
fn bpm_compatibility(a: Option<f64>, b: Option<f64>) -> f64 {
    match (a, b) {
        (Some(a), Some(b)) => 1.0 - (effective_bpm_diff(a, b) / MAX_BPM_DIFF).min(1.0),
        _ => 0.5,
    }
}

/// Harmonic compatibility in `0.0..=1.0`; unknown key yields neutral `0.5`.
fn key_compatibility(a: Option<CamelotKey>, b: Option<CamelotKey>) -> f64 {
    match (a, b) {
        (Some(a), Some(b)) => a.compatibility(b),
        _ => 0.5,
    }
}

/// Energy closeness in `0.0..=1.0`; unknown energy yields neutral `0.5`.
fn energy_closeness(a: Option<u8>, b: Option<u8>) -> f64 {
    match (a, b) {
        (Some(a), Some(b)) => 1.0 - (a as f64 - b as f64).abs() / 100.0,
        _ => 0.5,
    }
}

/// Alignment of the transition with the active mix goal, `0.0..=1.0`.
fn goal_alignment(from: &Track, to: &Track, goal: &MixGoal, genre_sim: f64) -> f64 {
    let de = match (from.energy, to.energy) {
        (Some(a), Some(b)) => Some(b as f64 - a as f64),
        _ => None,
    };
    let same_top_genre = genre_sim >= 0.8;
    match goal {
        MixGoal::Neutral => 0.5,
        MixGoal::RaiseEnergy => de.map_or(0.5, |d| (0.5 + d / 40.0).clamp(0.0, 1.0)),
        MixGoal::LowerEnergy => de.map_or(0.5, |d| (0.5 - d / 40.0).clamp(0.0, 1.0)),
        MixGoal::KeepGenre => {
            if same_top_genre {
                1.0
            } else {
                0.2
            }
        }
        MixGoal::SwitchGenre => {
            if same_top_genre {
                0.2
            } else {
                0.9
            }
        }
        MixGoal::HarmonicPriority => key_compatibility(from.key, to.key),
        MixGoal::SurprisingButPlausible => {
            // Reward a genre change that is still harmonically plausible.
            let surprise = 1.0 - genre_sim;
            let plausible = key_compatibility(from.key, to.key);
            (surprise * plausible).clamp(0.0, 1.0)
        }
        MixGoal::TargetTrack(id) => {
            if to.id == *id {
                1.0
            } else {
                0.5
            }
        }
        MixGoal::TargetGenre(g) => {
            if to.genre.as_deref() == Some(g.as_str()) || to.subgenre.as_deref() == Some(g.as_str())
            {
                1.0
            } else {
                0.5
            }
        }
    }
}

/// Builds German-language rationale lines matching the design's wording.
fn build_notes(bpm: f64, key: f64, energy: f64, genre: f64, bpm_diff: Option<f64>) -> Vec<String> {
    let mut notes = Vec::new();
    if let Some(d) = bpm_diff {
        let rd = d.round();
        if rd.abs() <= 8.0 {
            notes.push(format!("Kontrollierter Tempo-Shift ({rd:+} BPM)"));
        } else {
            notes.push(format!("Großer Tempo-Sprung ({rd:+} BPM) — Vorsicht"));
        }
    }
    if key >= 0.85 {
        notes.push("Harmonisch kompatible Tonart".to_string());
    } else if key <= 0.3 {
        notes.push("Tonarten passen nur bedingt".to_string());
    }
    if energy >= 0.9 {
        notes.push("Gleiche Energie".to_string());
    }
    if genre >= 0.8 {
        notes.push("Passendes Klangbild".to_string());
    } else if genre <= 0.3 {
        notes.push("Deutlicher Genrewechsel".to_string());
    }
    let _ = bpm; // bpm sub-score reflected via bpm_diff note above.
    notes
}

/// Scores the transition from `from` to `to` under the given weights and goal.
///
/// `played` is the set of already-played track ids used for the repetition
/// penalty. Pass an empty set when that is irrelevant.
pub fn score_transition(
    from: &Track,
    to: &Track,
    weights: &TransitionWeights,
    goal: &MixGoal,
    genres: &GenreContext,
    played: &HashSet<i64>,
) -> TransitionScore {
    let bpm = bpm_compatibility(from.bpm, to.bpm);
    let key = key_compatibility(from.key, to.key);
    let energy = energy_closeness(from.energy, to.energy);
    let genre = genres.similarity(from, to);
    // Mixability = feasibility of a clean beatmatched blend: dominated by BPM
    // proximity, improved by harmonic compatibility.
    let mixability = (bpm * (0.5 + 0.5 * key)).clamp(0.0, 1.0);
    let goal_score = goal_alignment(from, to, goal, genre);
    let rep = if played.contains(&to.id) { 1.0 } else { 0.0 };

    let bpm_diff = match (from.bpm, to.bpm) {
        (Some(a), Some(b)) => Some(b - a),
        _ => None,
    };

    let w = weights;
    let reward = w.bpm * bpm
        + w.key * key
        + w.energy * energy
        + w.genre * genre
        + w.mixability * mixability
        + w.goal * goal_score;
    let denom = w.positive_sum().max(f64::EPSILON);
    let total = ((reward - w.repetition_penalty * rep) / denom).clamp(0.0, 1.0);

    TransitionScore {
        total,
        bpm,
        key,
        energy,
        genre,
        mixability,
        goal: goal_score,
        repetition_penalty: rep,
        bpm_diff,
        notes: build_notes(bpm, key, energy, genre, bpm_diff),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::default_genres;
    use crate::sample_data::sample_tracks;

    fn ctx() -> GenreContext {
        GenreContext::from_genres(&default_genres())
    }

    fn track(id: i64, genre: &str, sub: &str, bpm: f64, key: &str, energy: u8) -> Track {
        let mut t = Track::new_from_path(format!("/{id}.mp3"), format!("T{id}"), 0);
        t.id = id;
        t.genre = Some(genre.into());
        t.subgenre = Some(sub.into());
        t.bpm = Some(bpm);
        t.key = Some(key.parse().unwrap());
        t.energy = Some(energy);
        t
    }

    #[test]
    fn identical_track_scores_high() {
        let a = track(1, "techno", "peak-time", 138.0, "8A", 85);
        let s = score_transition(
            &a,
            &a,
            &TransitionWeights::default(),
            &MixGoal::Neutral,
            &ctx(),
            &HashSet::new(),
        );
        assert!(s.bpm == 1.0 && s.key == 1.0 && s.energy == 1.0 && s.genre == 1.0);
        assert!(s.total > 0.8);
    }

    #[test]
    fn score_is_normalized_0_1() {
        let tracks = sample_tracks();
        let g = ctx();
        let w = TransitionWeights::default();
        for a in &tracks {
            for b in &tracks {
                let s = score_transition(a, b, &w, &MixGoal::Neutral, &g, &HashSet::new());
                assert!(
                    (0.0..=1.0).contains(&s.total),
                    "total out of range: {}",
                    s.total
                );
                for v in [s.bpm, s.key, s.energy, s.genre, s.mixability, s.goal] {
                    assert!((0.0..=1.0).contains(&v));
                }
            }
        }
    }

    #[test]
    fn half_time_bpm_is_compatible() {
        let a = track(1, "techno", "peak-time", 140.0, "8A", 80);
        let b = track(2, "house", "deep-house", 70.0, "8A", 80);
        let s = score_transition(
            &a,
            &b,
            &TransitionWeights::default(),
            &MixGoal::Neutral,
            &ctx(),
            &HashSet::new(),
        );
        assert!(
            s.bpm > 0.9,
            "half-time should be highly compatible: {}",
            s.bpm
        );
    }

    #[test]
    fn repetition_penalty_lowers_score() {
        let a = track(1, "techno", "peak-time", 138.0, "8A", 85);
        let b = track(2, "techno", "peak-time", 138.0, "8A", 85);
        let w = TransitionWeights::default();
        let fresh = score_transition(&a, &b, &w, &MixGoal::Neutral, &ctx(), &HashSet::new());
        let played: HashSet<i64> = [2].into_iter().collect();
        let repeated = score_transition(&a, &b, &w, &MixGoal::Neutral, &ctx(), &played);
        assert!(repeated.total < fresh.total);
    }

    #[test]
    fn raise_energy_goal_prefers_higher_energy() {
        let a = track(1, "techno", "peak-time", 138.0, "8A", 60);
        let higher = track(2, "techno", "peak-time", 138.0, "8A", 85);
        let lower = track(3, "techno", "peak-time", 138.0, "8A", 40);
        let w = TransitionWeights::default();
        let g = ctx();
        let up = score_transition(&a, &higher, &w, &MixGoal::RaiseEnergy, &g, &HashSet::new());
        let down = score_transition(&a, &lower, &w, &MixGoal::RaiseEnergy, &g, &HashSet::new());
        assert!(up.goal > down.goal);
    }

    #[test]
    fn unknown_bpm_is_neutral_not_zero() {
        let mut a = track(1, "techno", "peak-time", 138.0, "8A", 85);
        a.bpm = None;
        let b = track(2, "techno", "peak-time", 138.0, "8A", 85);
        let s = score_transition(
            &a,
            &b,
            &TransitionWeights::default(),
            &MixGoal::Neutral,
            &ctx(),
            &HashSet::new(),
        );
        assert_eq!(s.bpm, 0.5);
        assert!(s.bpm_diff.is_none());
    }
}
