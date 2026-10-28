//! Configuration: scoring weights, mix goals and the default genre taxonomy.
//!
//! Weights are deliberately data-driven and serializable so the UI (and, later,
//! the learning system in spec §9) can tune them without code changes.

use crate::models::Genre;
use serde::{Deserialize, Serialize};

/// Weights for the multi-dimensional [`TransitionScore`](crate::transition_engine::TransitionScore).
///
/// Mirrors the formula in spec §4:
/// ```text
/// TransitionScore = w_bpm·BPM + w_key·Harmonic + w_energy·Energy
///                 + w_genre·Genre + w_mix·Mixability + w_goal·GoalAlignment
///                 − w_repeat·RepetitionPenalty
/// ```
///
/// All sub-scores are normalized to `0.0..=1.0`; the final score is normalized
/// by the sum of the positive weights so it, too, lands in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TransitionWeights {
    /// Weight of BPM compatibility.
    pub bpm: f64,
    /// Weight of harmonic (key) compatibility.
    pub key: f64,
    /// Weight of energy closeness.
    pub energy: f64,
    /// Weight of genre/style similarity.
    pub genre: f64,
    /// Weight of overall mixability.
    pub mixability: f64,
    /// Weight of alignment with the active mix goal.
    pub goal: f64,
    /// Penalty weight for repeating an already-played track.
    pub repetition_penalty: f64,
}

impl Default for TransitionWeights {
    fn default() -> Self {
        // Balanced starting point; BPM and key dominate, goal nudges direction.
        TransitionWeights {
            bpm: 1.0,
            key: 1.0,
            energy: 0.8,
            genre: 0.7,
            mixability: 0.6,
            goal: 0.9,
            repetition_penalty: 1.5,
        }
    }
}

impl TransitionWeights {
    /// Sum of the positive (reward) weights, used to normalize the final score.
    pub fn positive_sum(&self) -> f64 {
        self.bpm + self.key + self.energy + self.genre + self.mixability + self.goal
    }
}

/// The musical direction the DJ wants the recommendation engine to pursue
/// (spec §4, the seven supported mix goals).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum MixGoal {
    /// No particular direction; reward smooth, close transitions.
    #[default]
    Neutral,
    /// Gradually increase energy.
    RaiseEnergy,
    /// Gradually decrease energy.
    LowerEnergy,
    /// Stay within the current genre.
    KeepGenre,
    /// Deliberately but controllably change genre.
    SwitchGenre,
    /// Prioritize harmonic (key) compatibility above all else.
    HarmonicPriority,
    /// Favor surprising yet plausible transitions.
    SurprisingButPlausible,
    /// Work towards a specific target track id.
    TargetTrack(i64),
    /// Work towards a specific target genre id.
    TargetGenre(String),
}

/// The full MixTree configuration persisted in the database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Configuration {
    /// Active scoring weights.
    pub weights: TransitionWeights,
    /// Active mix goal.
    pub goal: MixGoal,
    /// Maximum outgoing edges kept per node when building the graph
    /// (bounds graph density for large libraries, spec §3C).
    pub max_edges_per_node: usize,
    /// Minimum transition score for an edge to be shown at all.
    pub min_edge_score: f64,
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            weights: TransitionWeights::default(),
            goal: MixGoal::default(),
            max_edges_per_node: 4,
            min_edge_score: 0.35,
        }
    }
}

/// A row in the built-in genre table: `(id, name, parent, color, bpm_range)`.
type GenreRow = (
    &'static str,
    &'static str,
    Option<&'static str>,
    &'static str,
    Option<(u32, u32)>,
);

/// The built-in default genre taxonomy matching the design concept (spec §3B).
///
/// Users can override this at runtime; it only seeds a fresh database.
pub fn default_genres() -> Vec<Genre> {
    let rows: &[GenreRow] = &[
        ("techno", "Techno", None, "#ff2d55", Some((128, 142))),
        (
            "peak-time",
            "Peak Time",
            Some("techno"),
            "#ff2d55",
            Some((132, 140)),
        ),
        (
            "melodic-techno",
            "Melodic",
            Some("techno"),
            "#ff6b81",
            Some((120, 126)),
        ),
        (
            "hard-techno",
            "Hard Techno",
            Some("techno"),
            "#ff1f3d",
            Some((140, 150)),
        ),
        ("house", "House", None, "#2d9cff", Some((120, 130))),
        (
            "deep-house",
            "Deep House",
            Some("house"),
            "#2d9cff",
            Some((118, 124)),
        ),
        (
            "progressive-house",
            "Progressive",
            Some("house"),
            "#4db5ff",
            Some((124, 130)),
        ),
        (
            "tech-house",
            "Tech House",
            Some("house"),
            "#1f8fff",
            Some((122, 128)),
        ),
        ("trance", "Trance", None, "#2ecc71", Some((132, 142))),
        (
            "uplifting",
            "Uplifting",
            Some("trance"),
            "#2ecc71",
            Some((136, 142)),
        ),
        (
            "psytrance",
            "Psytrance",
            Some("trance"),
            "#27d17c",
            Some((140, 148)),
        ),
        (
            "dream-trance",
            "Dream Trance",
            Some("trance"),
            "#5be39b",
            Some((128, 134)),
        ),
        ("ambient", "Ambient", None, "#a855f7", Some((70, 110))),
        (
            "chillout",
            "Chillout",
            Some("ambient"),
            "#a855f7",
            Some((80, 100)),
        ),
        (
            "downtempo",
            "Downtempo",
            Some("ambient"),
            "#b76bff",
            Some((90, 110)),
        ),
        (
            "psybient",
            "Psybient",
            Some("ambient"),
            "#9333ea",
            Some((70, 95)),
        ),
        ("hardstyle", "Hardstyle", None, "#ff8c1a", Some((150, 170))),
        ("raw", "Raw", Some("hardstyle"), "#ff8c1a", Some((150, 160))),
        (
            "frenchcore",
            "Frenchcore",
            Some("hardstyle"),
            "#ff7700",
            Some((180, 200)),
        ),
        (
            "uptempo",
            "Uptempo",
            Some("hardstyle"),
            "#ff6600",
            Some((190, 220)),
        ),
        ("electro", "Electro", None, "#9aa5b1", Some((125, 130))),
        ("breakbeat", "Breakbeat", None, "#9aa5b1", Some((130, 140))),
        ("rock-indie", "Rock / Indie", None, "#9aa5b1", None),
    ];
    rows.iter()
        .enumerate()
        .map(|(i, (id, name, parent, color, bpm_range))| Genre {
            id: id.to_string(),
            name: name.to_string(),
            parent: parent.map(|p| p.to_string()),
            color: color.to_string(),
            bpm_range: *bpm_range,
            sort_order: i as i32,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sane() {
        let c = Configuration::default();
        assert!(c.weights.positive_sum() > 0.0);
        assert!(c.max_edges_per_node >= 1);
        assert!((0.0..=1.0).contains(&c.min_edge_score));
    }

    #[test]
    fn genre_taxonomy_parents_exist() {
        let genres = default_genres();
        let ids: std::collections::HashSet<_> = genres.iter().map(|g| g.id.as_str()).collect();
        for g in &genres {
            if let Some(p) = &g.parent {
                assert!(ids.contains(p.as_str()), "missing parent {p} for {}", g.id);
            }
        }
        // All spec top-level genres present.
        for top in [
            "techno",
            "house",
            "trance",
            "ambient",
            "hardstyle",
            "electro",
            "breakbeat",
            "rock-indie",
        ] {
            assert!(ids.contains(top), "missing top-level genre {top}");
        }
    }

    #[test]
    fn mix_goal_roundtrips_json() {
        for g in [
            MixGoal::Neutral,
            MixGoal::RaiseEnergy,
            MixGoal::TargetTrack(7),
            MixGoal::TargetGenre("house".into()),
        ] {
            let s = serde_json::to_string(&g).unwrap();
            let back: MixGoal = serde_json::from_str(&s).unwrap();
            assert_eq!(g, back);
        }
    }
}
