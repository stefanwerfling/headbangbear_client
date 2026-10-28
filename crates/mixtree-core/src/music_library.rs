//! Music library import: recursive folder scan, metadata extraction,
//! duplicate detection and persistence (spec §8).
//!
//! Import is resilient: a single unreadable or unsupported file never aborts
//! the whole run; it is recorded in [`ImportReport::errors`] and the scan
//! continues. BPM/key/energy are left for the Phase-2 analysis pipeline, which
//! import enqueues jobs for.

use crate::analysis::AnalysisKind;
use crate::database::{schema, MixtreeDb};
use crate::error::Result;
use crate::metadata::{self, is_supported_audio};
use crate::models::{Genre, Track};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Progress update emitted during an import (for the Tauri progress event).
#[derive(Debug, Clone)]
pub struct ImportProgress {
    /// Files processed so far.
    pub current: usize,
    /// Total files discovered.
    pub total: usize,
    /// Path currently being processed.
    pub path: String,
}

/// Summary of an import run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportReport {
    /// Newly inserted tracks.
    pub imported: usize,
    /// Existing tracks whose metadata was refreshed.
    pub updated: usize,
    /// Files skipped because their content hash already exists at another path.
    pub duplicates: usize,
    /// Per-file errors: `(path, message)`.
    pub errors: Vec<(String, String)>,
}

/// Builds a lookup from both genre id and lower-cased genre name to the
/// canonical taxonomy id, so free-text tags map onto configured genres.
fn genre_lookup(genres: &[Genre]) -> HashMap<String, (String, Option<String>)> {
    // key -> (genre_id, subgenre_id_if_key_is_a_subgenre)
    let mut map = HashMap::new();
    for g in genres {
        let top = top_level_id(genres, &g.id);
        let (genre_id, sub_id) = if g.parent.is_some() {
            (top.clone(), Some(g.id.clone()))
        } else {
            (g.id.clone(), None)
        };
        map.insert(g.id.to_lowercase(), (genre_id.clone(), sub_id.clone()));
        map.insert(g.name.to_lowercase(), (genre_id, sub_id));
    }
    map
}

fn top_level_id(genres: &[Genre], id: &str) -> String {
    let by_id: HashMap<&str, &Genre> = genres.iter().map(|g| (g.id.as_str(), g)).collect();
    let mut cur = id;
    for _ in 0..8 {
        match by_id.get(cur).and_then(|g| g.parent.as_deref()) {
            Some(parent) => cur = parent,
            None => break,
        }
    }
    cur.to_string()
}

/// Maps a free-text genre tag onto `(genre_id, subgenre_id)` from the taxonomy.
fn map_genre(
    tag: Option<&str>,
    lookup: &HashMap<String, (String, Option<String>)>,
) -> (Option<String>, Option<String>) {
    match tag {
        Some(t) => {
            let key = t.trim().to_lowercase();
            match lookup.get(&key) {
                Some((g, s)) => (Some(g.clone()), s.clone()),
                None => (None, None),
            }
        }
        None => (None, None),
    }
}

/// Recursively collects supported audio files under `root`.
fn collect_audio_files(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| is_supported_audio(p))
        .collect()
}

/// Imports every supported audio file under `root` into the library.
///
/// `now` is the import timestamp (unix seconds) stamped as `date_added`.
/// `progress` is invoked once per file. Returns an [`ImportReport`].
pub fn import_folder(
    db: &MixtreeDb,
    root: impl AsRef<Path>,
    now: i64,
    mut progress: impl FnMut(ImportProgress),
) -> Result<ImportReport> {
    let genres = db.list_genres()?;
    let lookup = genre_lookup(&genres);

    let files = collect_audio_files(root.as_ref());
    let total = files.len();
    let mut report = ImportReport::default();

    for (i, path) in files.into_iter().enumerate() {
        let path_str = path.display().to_string();
        progress(ImportProgress {
            current: i + 1,
            total,
            path: path_str.clone(),
        });

        match import_one(db, &path, now, &lookup) {
            Ok(Outcome::Imported) => report.imported += 1,
            Ok(Outcome::Updated) => report.updated += 1,
            Ok(Outcome::Duplicate) => report.duplicates += 1,
            Err(e) => report.errors.push((path_str, e.to_string())),
        }
    }

    Ok(report)
}

enum Outcome {
    Imported,
    Updated,
    Duplicate,
}

fn import_one(
    db: &MixtreeDb,
    path: &Path,
    now: i64,
    lookup: &HashMap<String, (String, Option<String>)>,
) -> Result<Outcome> {
    let path_str = path.display().to_string();
    let hash = metadata::content_hash(path)?;

    // Already imported at this exact path → refresh metadata.
    if let Some(mut existing) = db.find_track_by_path(&path_str)? {
        apply_metadata(&mut existing, path, lookup);
        existing.file_hash = Some(hash);
        existing.missing = false;
        db.update_track(&existing)?;
        return Ok(Outcome::Updated);
    }

    // Same content at a different path → duplicate (spec §8: dedup).
    if let Some(other) = db.find_track_by_hash(&hash)? {
        if Path::new(&other.file_path).exists() {
            return Ok(Outcome::Duplicate);
        }
        // Original is gone → treat this as the file having moved.
        let mut moved = other;
        moved.file_path = path_str;
        moved.missing = false;
        apply_metadata(&mut moved, path, lookup);
        db.update_track(&moved)?;
        return Ok(Outcome::Updated);
    }

    // New track.
    let title = metadata::title_from_path(path);
    let mut track = Track::new_from_path(path_str, title, now);
    track.file_hash = Some(hash);
    apply_metadata(&mut track, path, lookup);
    let id = db.insert_track(&track)?;

    // Queue Phase-2 analysis for features the tags did not already provide.
    if track.bpm.is_none() {
        db.enqueue_job(id, AnalysisKind::Bpm, schema::ANALYSIS_VERSION, now)?;
    }
    if track.key.is_none() {
        db.enqueue_job(id, AnalysisKind::Key, schema::ANALYSIS_VERSION, now)?;
    }
    if track.energy.is_none() {
        db.enqueue_job(id, AnalysisKind::Energy, schema::ANALYSIS_VERSION, now)?;
    }
    // Always analyze structure (drops/sections) for newly imported tracks.
    db.enqueue_job(id, AnalysisKind::Structure, schema::ANALYSIS_VERSION, now)?;

    Ok(Outcome::Imported)
}

/// Reads tags from `path` and applies them to `track` (keeping an existing
/// title if tags lack one).
fn apply_metadata(
    track: &mut Track,
    path: &Path,
    lookup: &HashMap<String, (String, Option<String>)>,
) {
    if let Ok(meta) = metadata::extract(path) {
        if let Some(t) = meta.title {
            track.title = t;
        }
        track.artist = meta.artist.or_else(|| track.artist.clone());
        track.album = meta.album.or_else(|| track.album.clone());
        track.year = meta.year.or(track.year);
        track.duration_ms = meta.duration_ms.or(track.duration_ms);
        // A BPM tag, if present, is a starting value (analysis may refine it).
        track.bpm = meta.bpm.or(track.bpm);
        let (genre, subgenre) = map_genre(meta.genre.as_deref(), lookup);
        if genre.is_some() {
            track.genre = genre;
            track.subgenre = subgenre;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::job_queue::JobStatus;
    use crate::database::TrackFilter;
    use std::io::Write;

    #[test]
    fn genre_mapping_handles_names_and_ids() {
        let genres = crate::configuration::default_genres();
        let lookup = genre_lookup(&genres);
        assert_eq!(
            map_genre(Some("Deep House"), &lookup),
            (Some("house".into()), Some("deep-house".into()))
        );
        assert_eq!(
            map_genre(Some("techno"), &lookup),
            (Some("techno".into()), None)
        );
        assert_eq!(map_genre(Some("Unknown Genre"), &lookup), (None, None));
        assert_eq!(map_genre(None, &lookup), (None, None));
    }

    #[test]
    fn import_empty_folder_reports_zero() {
        let db = MixtreeDb::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let report = import_folder(&db, dir.path(), 0, |_| {}).unwrap();
        assert_eq!(report, ImportReport::default());
    }

    #[test]
    fn import_skips_non_audio_and_handles_corrupt() {
        let db = MixtreeDb::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        // A non-audio file is ignored entirely.
        std::fs::File::create(dir.path().join("notes.txt"))
            .unwrap()
            .write_all(b"hello")
            .unwrap();
        // A corrupt "mp3": collected, but metadata extraction fails → error row,
        // yet the file is still inserted with a path-derived title.
        std::fs::File::create(dir.path().join("broken.mp3"))
            .unwrap()
            .write_all(b"not an mp3")
            .unwrap();

        let report = import_folder(&db, dir.path(), 123, |_| {}).unwrap();
        // broken.mp3 still imported (title from filename), extraction error tolerated.
        assert_eq!(report.imported, 1);
        let tracks = db.list_tracks(&TrackFilter::default()).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "broken");
        // Analysis jobs were enqueued for the missing features.
        assert!(db.count_jobs(JobStatus::Queued).unwrap() >= 1);
    }

    #[test]
    fn reimport_updates_not_duplicates() {
        let db = MixtreeDb::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::File::create(dir.path().join("a.flac"))
            .unwrap()
            .write_all(b"content-a")
            .unwrap();

        let r1 = import_folder(&db, dir.path(), 1, |_| {}).unwrap();
        assert_eq!(r1.imported, 1);
        let r2 = import_folder(&db, dir.path(), 2, |_| {}).unwrap();
        assert_eq!(r2.imported, 0);
        assert_eq!(r2.updated, 1);
        assert_eq!(db.list_tracks(&TrackFilter::default()).unwrap().len(), 1);
    }

    #[test]
    fn same_content_other_path_is_duplicate() {
        let db = MixtreeDb::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::File::create(dir.path().join("a.wav"))
            .unwrap()
            .write_all(b"identical-bytes")
            .unwrap();
        std::fs::File::create(dir.path().join("copy.wav"))
            .unwrap()
            .write_all(b"identical-bytes")
            .unwrap();
        let report = import_folder(&db, dir.path(), 1, |_| {}).unwrap();
        assert_eq!(report.imported, 1);
        assert_eq!(report.duplicates, 1);
    }

    #[test]
    fn progress_is_reported() {
        let db = MixtreeDb::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        for n in 0..3 {
            std::fs::File::create(dir.path().join(format!("t{n}.ogg")))
                .unwrap()
                .write_all(format!("bytes-{n}").as_bytes())
                .unwrap();
        }
        let mut seen = 0;
        let report = import_folder(&db, dir.path(), 1, |p| {
            seen += 1;
            assert_eq!(p.total, 3);
        })
        .unwrap();
        assert_eq!(seen, 3);
        assert_eq!(report.imported, 3);
    }
}
