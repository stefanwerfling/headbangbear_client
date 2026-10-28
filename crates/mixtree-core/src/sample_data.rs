//! Development sample data (the "Beispieldaten" required for Phase 1, spec §12).
//!
//! **This data is clearly marked and for development only.** The real app
//! imports, analyzes and plays actual audio files. These tracks mirror the
//! MixTree design concept so the graph, red path and recommendation panel can
//! be exercised end-to-end before the analysis pipeline (Phase 2) exists.

use crate::camelot::CamelotKey;
use crate::models::{AnalysisState, Track};

/// Marker embedded in every sample track's `file_path` so sample data is never
/// confused with a real imported file.
pub const SAMPLE_PATH_PREFIX: &str = "sample://";

/// A sample-track row: `(id, title, artist, genre, subgenre, bpm, key, energy)`.
type SampleRow = (
    i64,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    f64,
    &'static str,
    u8,
);

/// Returns the built-in sample tracks that reproduce the design concept.
pub fn sample_tracks() -> Vec<Track> {
    let rows: &[SampleRow] = &[
        (
            1,
            "Hypnotic Flow",
            "Nocturne",
            "techno",
            "peak-time",
            138.0,
            "8A",
            85,
        ),
        (
            2,
            "Rising Sun",
            "Aurora",
            "techno",
            "peak-time",
            132.0,
            "9A",
            78,
        ),
        (
            3,
            "Dark Matter",
            "Void",
            "techno",
            "hard-techno",
            140.0,
            "7A",
            92,
        ),
        (
            4,
            "Deep Horizon",
            "Tidal",
            "house",
            "deep-house",
            128.0,
            "8A",
            82,
        ),
        (
            5,
            "Night Drive",
            "Neon",
            "house",
            "tech-house",
            126.0,
            "8B",
            74,
        ),
        (
            6,
            "Lost in Time",
            "Echoes",
            "house",
            "progressive-house",
            130.0,
            "7A",
            68,
        ),
        (
            7,
            "Skyline",
            "Highrise",
            "trance",
            "uplifting",
            142.0,
            "9A",
            76,
        ),
        (
            8,
            "Euphoria",
            "Lift",
            "trance",
            "uplifting",
            145.0,
            "9B",
            88,
        ),
        (
            9,
            "Beyond",
            "Horizon",
            "trance",
            "psytrance",
            148.0,
            "10A",
            93,
        ),
        (
            10,
            "Morning Light",
            "Dawn",
            "ambient",
            "chillout",
            100.0,
            "5A",
            45,
        ),
        (
            11,
            "Floating",
            "Drift",
            "ambient",
            "downtempo",
            110.0,
            "6A",
            60,
        ),
        (
            12,
            "Last Horizon",
            "Dusk",
            "ambient",
            "downtempo",
            118.0,
            "6B",
            72,
        ),
        (
            13,
            "Raw Energy",
            "Hammer",
            "hardstyle",
            "raw",
            152.0,
            "11A",
            82,
        ),
        (
            14,
            "Unstoppable",
            "Rage",
            "hardstyle",
            "raw",
            160.0,
            "11B",
            90,
        ),
        (
            15,
            "Inferno",
            "Blaze",
            "hardstyle",
            "uptempo",
            168.0,
            "12A",
            95,
        ),
    ];
    rows.iter()
        .map(|&(id, title, artist, genre, sub, bpm, key, energy)| {
            let mut t = Track::new_from_path(format!("{SAMPLE_PATH_PREFIX}{id}"), title, 0);
            t.id = id;
            t.artist = Some(artist.to_string());
            t.genre = Some(genre.to_string());
            t.subgenre = Some(sub.to_string());
            t.bpm = Some(bpm);
            t.key = Some(key.parse::<CamelotKey>().expect("valid sample key"));
            t.energy = Some(energy);
            t.duration_ms = Some(6 * 60 * 1000);
            t.analysis_state = AnalysisState::Done;
            t.analysis_version = 1;
            t
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_tracks_are_complete() {
        let tracks = sample_tracks();
        assert_eq!(tracks.len(), 15);
        for t in &tracks {
            assert!(t.bpm.is_some() && t.key.is_some() && t.energy.is_some());
            assert!(t.file_path.starts_with(SAMPLE_PATH_PREFIX));
            assert!((0..=100).contains(&t.energy.unwrap()));
        }
    }
}
