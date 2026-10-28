//! # MixTree Core
//!
//! UI- and audio-independent core of the MixTree DJ application (spec §10).
//! It owns the data models, the local SQLite library, the music graph, the
//! transition/scoring engine, pathfinding (the "red path") and the
//! recommendation engine, plus the Phase-2 analysis **interfaces**.
//!
//! Nothing here depends on Tauri, a GUI or an audio backend, so the whole crate
//! is exercised by `cargo test` on any machine. The Tauri shell (`src-tauri`)
//! is a thin command layer over these modules.
//!
//! ## Module map (mirrors spec §10)
//! - [`models`] — Track, Genre, Playlist, history, enums
//! - [`camelot`] — Camelot-wheel keys and harmonic compatibility
//! - [`configuration`] — weights, mix goals, default genre taxonomy
//! - [`database`] — SQLite persistence and the analysis job queue
//! - [`metadata`] — tag extraction and content hashing
//! - [`music_library`] — recursive import / dedup
//! - [`transition_engine`] — multi-dimensional transition scoring
//! - [`music_graph`] — sparse graph of tracks + genre groups
//! - [`pathfinding`] — Dijkstra + goal-directed beam search (red path)
//! - [`recommendation`] — next-best-track with alternatives and rationale
//! - [`analysis`] — Phase-2 detector traits + persisted job queue
//! - [`sample_data`] — development "Beispieldaten" (clearly marked)

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod analysis;
pub mod beatmatch;
pub mod camelot;
pub mod configuration;
pub mod database;
pub mod error;
pub mod metadata;
pub mod models;
pub mod music_graph;
pub mod music_library;
pub mod pathfinding;
pub mod recommendation;
pub mod sample_data;
pub mod structure_detection;
pub mod transition_engine;

pub use error::{CoreError, Result};

/// Seeds an **empty** database with the development sample tracks so the graph
/// has data to show before any real import (Phase 1, spec §12).
///
/// Does nothing if the library already contains tracks, so real imports are
/// never polluted. Returns the number of sample tracks inserted.
pub fn seed_sample_tracks_if_empty(db: &database::MixtreeDb) -> Result<usize> {
    use database::TrackFilter;
    if !db.list_tracks(&TrackFilter::default())?.is_empty() {
        return Ok(0);
    }
    let mut count = 0;
    for track in sample_data::sample_tracks() {
        // Reset id so the DB assigns one; keep the `sample://` path marker.
        let mut t = track;
        t.id = 0;
        db.insert_track(&t)?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod integration_tests {
    //! End-to-end checks across modules (spec §14 step 5: a real workflow).
    use super::*;
    use configuration::{Configuration, MixGoal};

    #[test]
    fn seeded_library_produces_graph_path_and_recommendations() {
        let db = database::MixtreeDb::open_in_memory().unwrap();
        let inserted = seed_sample_tracks_if_empty(&db).unwrap();
        assert_eq!(inserted, 15);
        // Idempotent: second call seeds nothing.
        assert_eq!(seed_sample_tracks_if_empty(&db).unwrap(), 0);

        let tracks = db.list_tracks(&database::TrackFilter::default()).unwrap();
        let genres = db.list_genres().unwrap();
        let config = Configuration::default();

        // Graph with a recommended red path.
        let start = tracks[0].id;
        let (graph, path) = music_graph::build_graph_with_path(
            &tracks,
            &genres,
            &config,
            start,
            &MixGoal::RaiseEnergy,
        );
        assert!(!graph.nodes.is_empty());
        assert!(!graph.edges.is_empty());
        assert!(path.len() >= 2, "expected a multi-step red path");

        // Next-best recommendations exist and are explained.
        let current = &tracks[0];
        let recs = recommendation::next_best_tracks(
            current,
            &tracks,
            &genres,
            &MixGoal::Neutral,
            &config.weights,
            &std::collections::HashSet::new(),
            3,
        );
        assert_eq!(recs.len(), 3);
        assert!(recs.iter().all(|r| !r.reason.is_empty()));
    }
}
