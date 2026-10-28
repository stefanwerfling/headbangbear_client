//! SQLite-backed persistence for the MixTree library (spec §8, §10).
//!
//! [`MixtreeDb`] wraps a [`rusqlite::Connection`] and exposes typed repository
//! methods. The database is the only module that touches SQL; every other
//! module works with the plain models, which keeps the storage engine
//! replaceable behind this boundary.

pub mod schema;

use crate::configuration::Configuration;
use crate::error::{CoreError, Result};
use crate::models::{
    AnalysisState, CuePoint, Genre, MixEvent, MixHistoryEntry, Section, SectionKind, Track,
};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;

/// A filter for [`MixtreeDb::list_tracks`] (left-sidebar filtering, spec §3B).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TrackFilter {
    /// Free-text match against title/artist/album (case-insensitive).
    pub search: Option<String>,
    /// Restrict to these top-level genre ids or sub-genre ids (OR-combined).
    pub genres: Vec<String>,
    /// Minimum BPM (inclusive).
    pub bpm_min: Option<f64>,
    /// Maximum BPM (inclusive).
    pub bpm_max: Option<f64>,
    /// Minimum energy (inclusive).
    pub energy_min: Option<u8>,
    /// Maximum energy (inclusive).
    pub energy_max: Option<u8>,
    /// Exclude tracks whose file is missing.
    pub exclude_missing: bool,
}

/// The library database handle.
pub struct MixtreeDb {
    conn: Connection,
}

impl MixtreeDb {
    /// Opens (creating if needed) a database at `path`, runs migrations and
    /// seeds defaults on first use.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// Opens an in-memory database (used by tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self> {
        // WAL + a busy timeout let the UI and a background analysis thread (each
        // with its own connection to the same file) read/write concurrently
        // without spurious "database is locked" errors.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
        schema::migrate(&conn)?;
        let mut db = MixtreeDb { conn };
        db.seed_defaults()?;
        Ok(db)
    }

    /// Seeds the default genre taxonomy and configuration if absent.
    fn seed_defaults(&mut self) -> Result<()> {
        let genre_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM genres", [], |r| r.get(0))?;
        if genre_count == 0 {
            for g in crate::configuration::default_genres() {
                self.upsert_genre(&g)?;
            }
        }
        if self.get_meta("config")?.is_none() {
            self.set_config(&Configuration::default())?;
        }
        self.set_meta("analysis_version", &schema::ANALYSIS_VERSION.to_string())?;
        Ok(())
    }

    // --- meta key/value ----------------------------------------------------

    fn get_meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Returns the persisted configuration (or the default if unset).
    pub fn get_config(&self) -> Result<Configuration> {
        match self.get_meta("config")? {
            Some(json) => Ok(serde_json::from_str(&json)?),
            None => Ok(Configuration::default()),
        }
    }

    /// Persists the configuration.
    pub fn set_config(&self, config: &Configuration) -> Result<()> {
        self.set_meta("config", &serde_json::to_string(config)?)
    }

    // --- genres ------------------------------------------------------------

    /// Inserts or updates a genre.
    pub fn upsert_genre(&self, g: &Genre) -> Result<()> {
        self.conn.execute(
            "INSERT INTO genres(id, name, parent, color, bpm_low, bpm_high, sort_order)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name=excluded.name, parent=excluded.parent, color=excluded.color,
                bpm_low=excluded.bpm_low, bpm_high=excluded.bpm_high,
                sort_order=excluded.sort_order",
            params![
                g.id,
                g.name,
                g.parent,
                g.color,
                g.bpm_range.map(|(l, _)| l),
                g.bpm_range.map(|(_, h)| h),
                g.sort_order
            ],
        )?;
        Ok(())
    }

    /// Lists all genres ordered for display.
    pub fn list_genres(&self) -> Result<Vec<Genre>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, parent, color, bpm_low, bpm_high, sort_order
             FROM genres ORDER BY sort_order, name",
        )?;
        let rows = stmt
            .query_map([], |r| {
                let low: Option<u32> = r.get(4)?;
                let high: Option<u32> = r.get(5)?;
                Ok(Genre {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    parent: r.get(2)?,
                    color: r.get(3)?,
                    bpm_range: match (low, high) {
                        (Some(l), Some(h)) => Some((l, h)),
                        _ => None,
                    },
                    sort_order: r.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    // --- tracks ------------------------------------------------------------

    /// Inserts a new track (ignores `track.id`) and returns the assigned id.
    pub fn insert_track(&self, track: &Track) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO tracks(file_path, file_hash, title, artist, album, genre, subgenre,
                bpm, key, energy, duration_ms, year, rating, cover_art_path, date_added,
                analysis_version, analysis_state, missing)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
            params![
                track.file_path,
                track.file_hash,
                track.title,
                track.artist,
                track.album,
                track.genre,
                track.subgenre,
                track.bpm,
                track.key.map(|k| k.to_string()),
                track.energy,
                track.duration_ms.map(|v| v as i64),
                track.year,
                track.rating,
                track.cover_art_path,
                track.date_added,
                track.analysis_version,
                analysis_state_str(track.analysis_state),
                track.missing as i64,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Updates an existing track by `id`.
    pub fn update_track(&self, track: &Track) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE tracks SET file_path=?2, file_hash=?3, title=?4, artist=?5, album=?6,
                genre=?7, subgenre=?8, bpm=?9, key=?10, energy=?11, duration_ms=?12,
                year=?13, rating=?14, cover_art_path=?15, date_added=?16,
                analysis_version=?17, analysis_state=?18, missing=?19
             WHERE id=?1",
            params![
                track.id,
                track.file_path,
                track.file_hash,
                track.title,
                track.artist,
                track.album,
                track.genre,
                track.subgenre,
                track.bpm,
                track.key.map(|k| k.to_string()),
                track.energy,
                track.duration_ms.map(|v| v as i64),
                track.year,
                track.rating,
                track.cover_art_path,
                track.date_added,
                track.analysis_version,
                analysis_state_str(track.analysis_state),
                track.missing as i64,
            ],
        )?;
        if n == 0 {
            return Err(CoreError::NotFound(format!("track id {}", track.id)));
        }
        Ok(())
    }

    /// Fetches a single track by id.
    pub fn get_track(&self, id: i64) -> Result<Option<Track>> {
        Ok(self
            .conn
            .query_row("SELECT * FROM tracks WHERE id = ?1", [id], row_to_track)
            .optional()?)
    }

    /// Finds a track by its file path (used for incremental re-import).
    pub fn find_track_by_path(&self, path: &str) -> Result<Option<Track>> {
        Ok(self
            .conn
            .query_row(
                "SELECT * FROM tracks WHERE file_path = ?1",
                [path],
                row_to_track,
            )
            .optional()?)
    }

    /// Finds a track by its content hash (duplicate / moved-file detection).
    pub fn find_track_by_hash(&self, hash: &str) -> Result<Option<Track>> {
        Ok(self
            .conn
            .query_row(
                "SELECT * FROM tracks WHERE file_hash = ?1",
                [hash],
                row_to_track,
            )
            .optional()?)
    }

    /// Deletes a track by id.
    pub fn delete_track(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM tracks WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Lists tracks matching `filter`, ordered by artist/title.
    ///
    /// The query is built with bound parameters only (no string interpolation
    /// of user input) to avoid SQL injection.
    pub fn list_tracks(&self, filter: &TrackFilter) -> Result<Vec<Track>> {
        let mut sql = String::from("SELECT * FROM tracks WHERE 1=1");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // Appends `value`, returns its 1-based placeholder index.
        macro_rules! bind {
            ($value:expr) => {{
                args.push(Box::new($value));
                args.len()
            }};
        }

        if let Some(search) = filter.search.as_ref().filter(|s| !s.trim().is_empty()) {
            let like = format!("%{}%", search.trim());
            let p1 = bind!(like.clone());
            let p2 = bind!(like.clone());
            let p3 = bind!(like);
            sql.push_str(&format!(
                " AND (title LIKE ?{p1} OR artist LIKE ?{p2} OR album LIKE ?{p3})"
            ));
        }
        if !filter.genres.is_empty() {
            // Bind each genre twice: once for the `genre` IN-list, once for the
            // `subgenre` IN-list.
            let genre_ph: Vec<String> = filter
                .genres
                .iter()
                .map(|g| format!("?{}", bind!(g.clone())))
                .collect();
            let sub_ph: Vec<String> = filter
                .genres
                .iter()
                .map(|g| format!("?{}", bind!(g.clone())))
                .collect();
            sql.push_str(&format!(
                " AND (genre IN ({}) OR subgenre IN ({}))",
                genre_ph.join(","),
                sub_ph.join(",")
            ));
        }
        if let Some(v) = filter.bpm_min {
            sql.push_str(&format!(" AND bpm >= ?{}", bind!(v)));
        }
        if let Some(v) = filter.bpm_max {
            sql.push_str(&format!(" AND bpm <= ?{}", bind!(v)));
        }
        if let Some(v) = filter.energy_min {
            sql.push_str(&format!(" AND energy >= ?{}", bind!(v as i64)));
        }
        if let Some(v) = filter.energy_max {
            sql.push_str(&format!(" AND energy <= ?{}", bind!(v as i64)));
        }
        if filter.exclude_missing {
            sql.push_str(" AND missing = 0");
        }
        sql.push_str(" ORDER BY artist, title");

        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
        let rows = stmt
            .query_map(params.as_slice(), row_to_track)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Marks tracks whose file no longer exists as `missing` and returns the count.
    pub fn refresh_missing(&self) -> Result<usize> {
        let tracks = self.list_tracks(&TrackFilter::default())?;
        let mut changed = 0;
        for mut t in tracks {
            if t.file_path
                .starts_with(crate::sample_data::SAMPLE_PATH_PREFIX)
            {
                continue; // sample tracks have no real file
            }
            let exists = Path::new(&t.file_path).exists();
            if exists == t.missing {
                t.missing = !exists;
                self.update_track(&t)?;
                changed += 1;
            }
        }
        Ok(changed)
    }

    // --- mix history -------------------------------------------------------

    /// Records a mix-history / feedback event (spec §9).
    pub fn add_history(
        &self,
        from: Option<i64>,
        to: Option<i64>,
        event: MixEvent,
        at: i64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO mix_history(from_track, to_track, event, at) VALUES(?1,?2,?3,?4)",
            params![from, to, mix_event_str(event), at],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Returns the most recent history entries, newest first.
    pub fn recent_history(&self, limit: usize) -> Result<Vec<MixHistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_track, to_track, event, at FROM mix_history ORDER BY at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([limit as i64], |r| {
                Ok(MixHistoryEntry {
                    id: r.get(0)?,
                    from_track: r.get(1)?,
                    to_track: r.get(2)?,
                    event: mix_event_from_str(&r.get::<_, String>(3)?),
                    at: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    // --- structure sections (spec §7) --------------------------------------

    /// Replaces all detected sections for a track with the given set.
    pub fn replace_sections(&self, track_id: i64, sections: &[Section]) -> Result<()> {
        self.conn
            .execute("DELETE FROM sections WHERE track_id = ?1", [track_id])?;
        for s in sections {
            self.conn.execute(
                "INSERT INTO sections(track_id, start_ms, end_ms, kind, confidence)
                 VALUES(?1, ?2, ?3, ?4, ?5)",
                params![
                    track_id,
                    s.start_ms as i64,
                    s.end_ms as i64,
                    s.kind.as_str(),
                    s.confidence
                ],
            )?;
        }
        Ok(())
    }

    /// Lists the detected sections for a track, ordered by start.
    pub fn list_sections(&self, track_id: i64) -> Result<Vec<Section>> {
        let mut stmt = self.conn.prepare(
            "SELECT start_ms, end_ms, kind, confidence FROM sections
             WHERE track_id = ?1 ORDER BY start_ms",
        )?;
        let rows = stmt
            .query_map([track_id], |r| {
                Ok(Section {
                    start_ms: r.get::<_, i64>(0)? as u64,
                    end_ms: r.get::<_, i64>(1)? as u64,
                    kind: SectionKind::from_str(&r.get::<_, String>(2)?),
                    confidence: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    // --- cue points (spec §6) ----------------------------------------------

    /// Adds a cue point and returns its id.
    pub fn add_cue(&self, track_id: i64, cue: &CuePoint) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO cue_points(track_id, position_ms, label, kind, color)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                track_id,
                cue.position_ms as i64,
                cue.label,
                cue.kind,
                cue.color
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Lists cue points for a track, ordered by position.
    pub fn list_cues(&self, track_id: i64) -> Result<Vec<CuePoint>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, position_ms, label, kind, color FROM cue_points
             WHERE track_id = ?1 ORDER BY position_ms",
        )?;
        let rows = stmt
            .query_map([track_id], |r| {
                Ok(CuePoint {
                    id: r.get(0)?,
                    position_ms: r.get::<_, i64>(1)? as u64,
                    label: r.get(2)?,
                    kind: r.get(3)?,
                    color: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Removes a track's suggested (structural) cues, keeping manual hot-cues.
    pub fn clear_structural_cues(&self, track_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM cue_points WHERE track_id = ?1 AND kind = 'structural'",
            [track_id],
        )?;
        Ok(())
    }

    /// Exposes the raw connection for the analysis job queue module.
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

// --- row / enum mapping helpers --------------------------------------------

fn row_to_track(r: &Row) -> rusqlite::Result<Track> {
    let key_str: Option<String> = r.get("key")?;
    let state_str: String = r.get("analysis_state")?;
    Ok(Track {
        id: r.get("id")?,
        file_path: r.get("file_path")?,
        file_hash: r.get("file_hash")?,
        title: r.get("title")?,
        artist: r.get("artist")?,
        album: r.get("album")?,
        genre: r.get("genre")?,
        subgenre: r.get("subgenre")?,
        bpm: r.get("bpm")?,
        key: key_str.and_then(|s| s.parse().ok()),
        energy: r.get::<_, Option<i64>>("energy")?.map(|v| v as u8),
        duration_ms: r.get::<_, Option<i64>>("duration_ms")?.map(|v| v as u64),
        year: r.get("year")?,
        rating: r.get::<_, Option<i64>>("rating")?.map(|v| v as u8),
        cover_art_path: r.get("cover_art_path")?,
        date_added: r.get("date_added")?,
        analysis_version: r.get("analysis_version")?,
        analysis_state: analysis_state_from_str(&state_str),
        missing: r.get::<_, i64>("missing")? != 0,
    })
}

fn analysis_state_str(s: AnalysisState) -> &'static str {
    match s {
        AnalysisState::Pending => "pending",
        AnalysisState::Queued => "queued",
        AnalysisState::Running => "running",
        AnalysisState::Done => "done",
        AnalysisState::Failed => "failed",
    }
}

fn analysis_state_from_str(s: &str) -> AnalysisState {
    match s {
        "queued" => AnalysisState::Queued,
        "running" => AnalysisState::Running,
        "done" => AnalysisState::Done,
        "failed" => AnalysisState::Failed,
        _ => AnalysisState::Pending,
    }
}

fn mix_event_str(e: MixEvent) -> &'static str {
    match e {
        MixEvent::SuggestionAccepted => "suggestion_accepted",
        MixEvent::SuggestionSkipped => "suggestion_skipped",
        MixEvent::TransitionEdited => "transition_edited",
        MixEvent::TransitionCompleted => "transition_completed",
        MixEvent::ManuallyPreferred => "manually_preferred",
        MixEvent::Excluded => "excluded",
    }
}

fn mix_event_from_str(s: &str) -> MixEvent {
    match s {
        "suggestion_skipped" => MixEvent::SuggestionSkipped,
        "transition_edited" => MixEvent::TransitionEdited,
        "transition_completed" => MixEvent::TransitionCompleted,
        "manually_preferred" => MixEvent::ManuallyPreferred,
        "excluded" => MixEvent::Excluded,
        _ => MixEvent::SuggestionAccepted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camelot::CamelotKey;

    fn sample_track() -> Track {
        let mut t = Track::new_from_path("/music/test.mp3", "Test Track", 1000);
        t.artist = Some("Artist".into());
        t.genre = Some("techno".into());
        t.subgenre = Some("peak-time".into());
        t.bpm = Some(138.0);
        t.key = Some("8A".parse::<CamelotKey>().unwrap());
        t.energy = Some(85);
        t
    }

    #[test]
    fn migrate_and_seed() {
        let db = MixtreeDb::open_in_memory().unwrap();
        assert!(!db.list_genres().unwrap().is_empty());
        let cfg = db.get_config().unwrap();
        assert!(cfg.max_edges_per_node >= 1);
    }

    #[test]
    fn insert_get_update_delete_track() {
        let db = MixtreeDb::open_in_memory().unwrap();
        let id = db.insert_track(&sample_track()).unwrap();
        let mut got = db.get_track(id).unwrap().unwrap();
        assert_eq!(got.title, "Test Track");
        assert_eq!(got.key.unwrap().to_string(), "8A");

        got.rating = Some(5);
        got.energy = Some(90);
        db.update_track(&got).unwrap();
        assert_eq!(db.get_track(id).unwrap().unwrap().rating, Some(5));

        db.delete_track(id).unwrap();
        assert!(db.get_track(id).unwrap().is_none());
    }

    #[test]
    fn list_tracks_filters() {
        let db = MixtreeDb::open_in_memory().unwrap();
        let mut a = sample_track();
        a.title = "Alpha".into();
        a.bpm = Some(128.0);
        a.genre = Some("house".into());
        a.subgenre = Some("deep-house".into());
        db.insert_track(&a).unwrap();
        let mut b = sample_track();
        b.file_path = "/music/b.mp3".into();
        b.title = "Beta".into();
        b.bpm = Some(150.0);
        db.insert_track(&b).unwrap();

        // Search.
        let r = db
            .list_tracks(&TrackFilter {
                search: Some("alph".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "Alpha");

        // Genre filter (sub-genre match).
        let r = db
            .list_tracks(&TrackFilter {
                genres: vec!["deep-house".into()],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(r.len(), 1);

        // BPM range.
        let r = db
            .list_tracks(&TrackFilter {
                bpm_min: Some(140.0),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "Beta");
    }

    #[test]
    fn duplicate_path_is_rejected() {
        let db = MixtreeDb::open_in_memory().unwrap();
        db.insert_track(&sample_track()).unwrap();
        assert!(db.insert_track(&sample_track()).is_err());
    }

    #[test]
    fn history_roundtrip() {
        let db = MixtreeDb::open_in_memory().unwrap();
        let id = db.insert_track(&sample_track()).unwrap();
        db.add_history(Some(id), None, MixEvent::SuggestionAccepted, 10)
            .unwrap();
        db.add_history(Some(id), None, MixEvent::SuggestionSkipped, 20)
            .unwrap();
        let h = db.recent_history(10).unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].event, MixEvent::SuggestionSkipped); // newest first
    }

    #[test]
    fn empty_library_lists_nothing() {
        let db = MixtreeDb::open_in_memory().unwrap();
        assert!(db.list_tracks(&TrackFilter::default()).unwrap().is_empty());
    }
}
