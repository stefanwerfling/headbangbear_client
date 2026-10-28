//! Next-best-track recommendation (the right sidebar in the design, spec §3D).
//!
//! Given the current track and the active goal, rank candidate tracks by their
//! transition score and return a fully-explained recommendation plus ranked
//! alternatives. Transition *hints* (cue/loop/duration) are **clearly flagged
//! as estimates** until the Phase-5 structure analysis exists (spec §6).

use crate::configuration::{MixGoal, TransitionWeights};
use crate::models::{Genre, Track};
use crate::transition_engine::{score_transition, GenreContext, TransitionScore};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A single track recommendation with its scoring and transition plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recommendation {
    /// The recommended track's id.
    pub track_id: i64,
    /// Title (denormalized for convenient display).
    pub title: String,
    /// Artist.
    pub artist: Option<String>,
    /// Top-level genre id.
    pub genre: Option<String>,
    /// BPM.
    pub bpm: Option<f64>,
    /// Signed BPM difference to the current track (display hint).
    pub bpm_diff: Option<f64>,
    /// Camelot key string.
    pub key: Option<String>,
    /// Energy indicator.
    pub energy: Option<u8>,
    /// Full transition score and breakdown.
    pub score: TransitionScore,
    /// Estimated transition plan (flagged as estimate).
    pub plan: TransitionPlan,
    /// Why this track was recommended (and its trade-offs).
    pub reason: String,
}

/// A suggested transition plan. **All fields are estimates** until real
/// structure/cue detection lands (spec §6: show uncertainty, don't fake
/// precision).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionPlan {
    /// Suggested loop length in beats (e.g. 8).
    pub loop_beats: u32,
    /// Suggested transition duration in beats (e.g. 16).
    pub transition_beats: u32,
    /// Approximate transition duration in seconds, derived from the target BPM.
    pub approx_seconds: Option<f64>,
    /// Always `true` in Phase 1–3: these are heuristic estimates, not detected.
    pub estimated: bool,
}

fn make_plan(to: &Track) -> TransitionPlan {
    let transition_beats = 16;
    let loop_beats = 8;
    let approx_seconds = to.bpm.map(|bpm| transition_beats as f64 * 60.0 / bpm);
    TransitionPlan {
        loop_beats,
        transition_beats,
        approx_seconds,
        estimated: true,
    }
}

fn make_reason(score: &TransitionScore, goal: &MixGoal) -> String {
    let mut parts = score.notes.clone();
    let quality = (score.total * 100.0).round();
    let lead = match goal {
        MixGoal::RaiseEnergy => "Treibt die Energie nach oben",
        MixGoal::LowerEnergy => "Nimmt die Energie zurück",
        MixGoal::KeepGenre => "Bleibt im Genre",
        MixGoal::SwitchGenre => "Leitet den Genrewechsel ein",
        MixGoal::HarmonicPriority => "Harmonisch optimiert",
        MixGoal::SurprisingButPlausible => "Überraschend, aber tragfähig",
        MixGoal::TargetTrack(_) => "Führt zum Zieltrack",
        MixGoal::TargetGenre(_) => "Führt zum Zielgenre",
        MixGoal::Neutral => "Runder Übergang",
    };
    parts.insert(0, format!("{lead} ({quality:.0} % Übergangsqualität)"));
    parts.join(" · ")
}

/// Returns up to `limit` recommendations for the track after `current`,
/// best first. `played` tracks are penalized (not excluded) via the score.
pub fn next_best_tracks(
    current: &Track,
    candidates: &[Track],
    genres: &[Genre],
    goal: &MixGoal,
    weights: &TransitionWeights,
    played: &HashSet<i64>,
    limit: usize,
) -> Vec<Recommendation> {
    let ctx = GenreContext::from_genres(genres);
    let mut recs: Vec<Recommendation> = candidates
        .iter()
        .filter(|c| c.id != current.id)
        .map(|c| {
            let score = score_transition(current, c, weights, goal, &ctx, played);
            Recommendation {
                track_id: c.id,
                title: c.title.clone(),
                artist: c.artist.clone(),
                genre: c.genre.as_ref().map(|g| ctx.top_level(g)),
                bpm: c.bpm,
                bpm_diff: score.bpm_diff,
                key: c.key.map(|k| k.to_string()),
                energy: c.energy,
                reason: make_reason(&score, goal),
                plan: make_plan(c),
                score,
            }
        })
        .collect();

    recs.sort_by(|a, b| {
        b.score
            .total
            .partial_cmp(&a.score.total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    recs.truncate(limit);
    recs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::default_genres;
    use crate::sample_data::sample_tracks;

    #[test]
    fn recommendations_are_sorted_desc() {
        let tracks = sample_tracks();
        let genres = default_genres();
        let current = &tracks[0];
        let recs = next_best_tracks(
            current,
            &tracks,
            &genres,
            &MixGoal::Neutral,
            &TransitionWeights::default(),
            &HashSet::new(),
            5,
        );
        assert!(recs.len() <= 5 && !recs.is_empty());
        for w in recs.windows(2) {
            assert!(w[0].score.total >= w[1].score.total);
        }
        // Current track never recommends itself.
        assert!(recs.iter().all(|r| r.track_id != current.id));
    }

    #[test]
    fn hypnotic_flow_to_deep_horizon_is_harmonic_cross_genre() {
        // The design concept pitches Hypnotic Flow (1) → Deep Horizon (4) as a
        // *harmonic, equal-energy* cross-genre move. Under the Neutral goal a
        // closer same-genre track may rank higher (a 10-BPM gap is real), but
        // Deep Horizon must still be recommended and flagged as harmonically
        // perfect with matching energy.
        let tracks = sample_tracks();
        let genres = default_genres();
        let current = &tracks[0]; // Hypnotic Flow
        let recs = next_best_tracks(
            current,
            &tracks,
            &genres,
            &MixGoal::Neutral,
            &TransitionWeights::default(),
            &HashSet::new(),
            15,
        );
        let dh = recs
            .iter()
            .find(|r| r.track_id == 4)
            .expect("Deep Horizon should be recommended");
        assert_eq!(
            dh.score.key, 1.0,
            "identical key 8A → perfect harmonic match"
        );
        assert!(dh.score.energy >= 0.9, "near-equal energy (85 vs 82)");
        assert_eq!(dh.bpm_diff, Some(-10.0), "BPM delta is surfaced to the DJ");
    }

    #[test]
    fn switch_genre_goal_surfaces_deep_horizon() {
        // With an explicit "switch genre" goal, the harmonic cross-genre move to
        // Deep Horizon becomes a top pick — matching the design's framing.
        let tracks = sample_tracks();
        let genres = default_genres();
        let recs = next_best_tracks(
            &tracks[0],
            &tracks,
            &genres,
            &MixGoal::SwitchGenre,
            &TransitionWeights::default(),
            &HashSet::new(),
            5,
        );
        assert!(
            recs.iter().take(3).any(|r| r.track_id == 4),
            "Deep Horizon should be a top-3 pick when switching genre"
        );
    }

    #[test]
    fn plans_are_flagged_as_estimates() {
        let tracks = sample_tracks();
        let genres = default_genres();
        let recs = next_best_tracks(
            &tracks[0],
            &tracks,
            &genres,
            &MixGoal::Neutral,
            &TransitionWeights::default(),
            &HashSet::new(),
            3,
        );
        assert!(recs.iter().all(|r| r.plan.estimated));
    }
}
