//! Core data models shared by the database, the engines and the Tauri API.
//!
//! These types are the single source of truth for what a track, a genre, a
//! transition and a path are. They all implement `Serialize`/`Deserialize` so
//! they can cross the Tauri boundary unchanged.

use crate::camelot::CamelotKey;
use serde::{Deserialize, Serialize};

/// State of the local analysis pipeline for a track (Phase 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisState {
    /// No analysis has been run yet. BPM/key/energy may come from tags only.
    #[default]
    Pending,
    /// Analysis is queued in the background job queue.
    Queued,
    /// Analysis is currently running.
    Running,
    /// Analysis completed successfully.
    Done,
    /// Analysis failed (e.g. unreadable audio). See logs for details.
    Failed,
}

/// A single track in the music library.
///
/// `bpm`, `key` and `energy` are `Option` because they may be unknown until the
/// Phase-2 analysis pipeline (or the file's own tags) provides them. `energy`
/// is an **estimated indicator** in `0..=100`, never a ground-truth value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Database primary key. `0` for a not-yet-persisted track.
    pub id: i64,
    /// Absolute path to the audio file on disk.
    pub file_path: String,
    /// Content hash used for duplicate detection and moved-file resilience.
    pub file_hash: Option<String>,
    /// Display title (falls back to the file stem when tags are missing).
    pub title: String,
    /// Performing artist.
    pub artist: Option<String>,
    /// Album name.
    pub album: Option<String>,
    /// Top-level genre id (matches [`Genre::id`]).
    pub genre: Option<String>,
    /// Sub-genre id (matches a child [`Genre::id`]).
    pub subgenre: Option<String>,
    /// Beats per minute. `None` until detected or tagged.
    pub bpm: Option<f64>,
    /// Musical key on the Camelot wheel.
    pub key: Option<CamelotKey>,
    /// Estimated energy indicator, `0..=100`. **Estimate, not ground truth.**
    pub energy: Option<u8>,
    /// Track length in milliseconds.
    pub duration_ms: Option<u64>,
    /// Release year.
    pub year: Option<i32>,
    /// User rating, `0..=5`.
    pub rating: Option<u8>,
    /// Path to extracted cover-art image, if any.
    pub cover_art_path: Option<String>,
    /// Unix timestamp (seconds) when the track was imported.
    pub date_added: i64,
    /// Version of the analysis algorithm that produced bpm/key/energy.
    ///
    /// Lets changed algorithms trigger targeted re-analysis (spec §8).
    pub analysis_version: i32,
    /// State of the analysis pipeline for this track.
    pub analysis_state: AnalysisState,
    /// `true` when the file is no longer present at `file_path`.
    pub missing: bool,
}

impl Track {
    /// Builds a minimal track for a freshly discovered file. All analysis
    /// fields start empty; metadata extraction fills them in.
    pub fn new_from_path(
        file_path: impl Into<String>,
        title: impl Into<String>,
        date_added: i64,
    ) -> Self {
        Track {
            id: 0,
            file_path: file_path.into(),
            file_hash: None,
            title: title.into(),
            artist: None,
            album: None,
            genre: None,
            subgenre: None,
            bpm: None,
            key: None,
            energy: None,
            duration_ms: None,
            year: None,
            rating: None,
            cover_art_path: None,
            date_added,
            analysis_version: 0,
            analysis_state: AnalysisState::Pending,
            missing: false,
        }
    }
}

/// A configurable genre or sub-genre node for the left sidebar and graph colors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Genre {
    /// Stable identifier (kebab-case), e.g. `"techno"`, `"peak-time"`.
    pub id: String,
    /// Human-readable name, e.g. `"Peak Time"`.
    pub name: String,
    /// Parent genre id, or `None` for a top-level genre.
    pub parent: Option<String>,
    /// Hex color used for the genre group and its nodes, e.g. `"#ff2d55"`.
    pub color: String,
    /// Typical BPM range `[low, high]` shown on the group node, if known.
    pub bpm_range: Option<(u32, u32)>,
    /// Sort order within its level.
    pub sort_order: i32,
}

/// A detected song-structure section (spec §7). Boundaries and kind are
/// **estimates** — carry `confidence` and show uncertainty rather than fake
/// precision (spec §6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    /// Section start in milliseconds.
    pub start_ms: u64,
    /// Section end in milliseconds.
    pub end_ms: u64,
    /// Estimated section type.
    pub kind: SectionKind,
    /// Detection confidence, `0.0..=1.0`.
    pub confidence: f64,
}

/// The type of a song-structure section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    /// Low-energy opening.
    Intro,
    /// Rising energy leading into a drop.
    Build,
    /// High-energy peak section.
    Drop,
    /// Low-energy mid-track breakdown.
    Breakdown,
    /// Steady mid-energy body.
    Steady,
    /// Low-energy closing.
    Outro,
    /// A quiet passage.
    Quiet,
}

impl SectionKind {
    /// Stable string form for storage.
    pub fn as_str(self) -> &'static str {
        match self {
            SectionKind::Intro => "intro",
            SectionKind::Build => "build",
            SectionKind::Drop => "drop",
            SectionKind::Breakdown => "breakdown",
            SectionKind::Steady => "steady",
            SectionKind::Outro => "outro",
            SectionKind::Quiet => "quiet",
        }
    }

    /// Parses the stored string form (defaults to `Steady`).
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "intro" => SectionKind::Intro,
            "build" => SectionKind::Build,
            "drop" => SectionKind::Drop,
            "breakdown" => SectionKind::Breakdown,
            "outro" => SectionKind::Outro,
            "quiet" => SectionKind::Quiet,
            _ => SectionKind::Steady,
        }
    }
}

/// A cue point on a track (manual hot-cue or a suggested structural cue, spec §6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CuePoint {
    /// Database id (`0` for a not-yet-persisted cue).
    pub id: i64,
    /// Position in milliseconds.
    pub position_ms: u64,
    /// Optional label, e.g. `"Drop"`, `"Mix In"`.
    pub label: Option<String>,
    /// `"hot"` for user hot-cues, `"structural"` for suggestions.
    pub kind: String,
    /// Optional hex color.
    pub color: Option<String>,
}

/// A playlist (ordered collection of tracks).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    /// Database primary key.
    pub id: i64,
    /// Display name.
    pub name: String,
    /// Ordered track ids.
    pub track_ids: Vec<i64>,
    /// Unix timestamp (seconds) of creation.
    pub created_at: i64,
}

/// One recorded event in the mix history / learning system (spec §9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixHistoryEntry {
    /// Database primary key.
    pub id: i64,
    /// Track that was playing.
    pub from_track: Option<i64>,
    /// Track that was mixed into.
    pub to_track: Option<i64>,
    /// Type of event (see [`MixEvent`]).
    pub event: MixEvent,
    /// Unix timestamp (seconds).
    pub at: i64,
}

/// Explicit, auditable feedback events for the learning system.
///
/// An *accepted suggestion is not automatically a successful mix* (spec §9),
/// hence distinct variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MixEvent {
    /// A suggested track was accepted by the DJ.
    SuggestionAccepted,
    /// A suggested track was skipped.
    SuggestionSkipped,
    /// The DJ manually changed a transition.
    TransitionEdited,
    /// A transition was played through to completion.
    TransitionCompleted,
    /// A track was manually preferred (not suggested).
    ManuallyPreferred,
    /// A track or genre was excluded.
    Excluded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_from_path_defaults() {
        let t = Track::new_from_path("/music/a.mp3", "A", 42);
        assert_eq!(t.id, 0);
        assert_eq!(t.analysis_state, AnalysisState::Pending);
        assert!(t.bpm.is_none());
        assert!(!t.missing);
        assert_eq!(t.date_added, 42);
    }

    #[test]
    fn analysis_state_serializes_snake_case() {
        let json = serde_json::to_string(&AnalysisState::Done).unwrap();
        assert_eq!(json, "\"done\"");
    }
}
