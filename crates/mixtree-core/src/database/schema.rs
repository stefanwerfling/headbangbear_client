//! Versioned SQLite schema and forward migrations (spec §8, §10).
//!
//! Migrations are an ordered list; the current `user_version` pragma tracks how
//! many have been applied so upgrades are incremental and reproducible. Analysis
//! results carry an `analysis_version` so changed algorithms can trigger
//! targeted recomputation.

use crate::error::Result;
use rusqlite::Connection;

/// Bumped whenever a new migration is appended to [`MIGRATIONS`].
pub const SCHEMA_VERSION: i64 = 2;

/// The current analysis-algorithm version. Tracks analyzed with an older
/// version are candidates for re-analysis in Phase 2.
pub const ANALYSIS_VERSION: i32 = 1;

/// Ordered DDL migrations. Index + 1 == the `user_version` after applying it.
const MIGRATIONS: &[&str] = &[
    // --- v1: initial schema -------------------------------------------------
    r#"
    CREATE TABLE meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE genres (
        id         TEXT PRIMARY KEY,
        name       TEXT NOT NULL,
        parent     TEXT REFERENCES genres(id) ON DELETE SET NULL,
        color      TEXT NOT NULL,
        bpm_low    INTEGER,
        bpm_high   INTEGER,
        sort_order INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE tracks (
        id               INTEGER PRIMARY KEY AUTOINCREMENT,
        file_path        TEXT NOT NULL UNIQUE,
        file_hash        TEXT,
        title            TEXT NOT NULL,
        artist           TEXT,
        album            TEXT,
        genre            TEXT,
        subgenre         TEXT,
        bpm              REAL,
        key              TEXT,
        energy           INTEGER,
        duration_ms      INTEGER,
        year             INTEGER,
        rating           INTEGER,
        cover_art_path   TEXT,
        date_added       INTEGER NOT NULL,
        analysis_version INTEGER NOT NULL DEFAULT 0,
        analysis_state   TEXT NOT NULL DEFAULT 'pending',
        missing          INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX idx_tracks_genre  ON tracks(genre);
    CREATE INDEX idx_tracks_bpm    ON tracks(bpm);
    CREATE INDEX idx_tracks_hash   ON tracks(file_hash);

    CREATE TABLE cue_points (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        position_ms INTEGER NOT NULL,
        label       TEXT,
        kind        TEXT NOT NULL DEFAULT 'hot',
        color       TEXT
    );
    CREATE INDEX idx_cue_track ON cue_points(track_id);

    CREATE TABLE loops (
        id        INTEGER PRIMARY KEY AUTOINCREMENT,
        track_id  INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        start_ms  INTEGER NOT NULL,
        end_ms    INTEGER NOT NULL,
        beats     INTEGER,
        active    INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX idx_loop_track ON loops(track_id);

    CREATE TABLE playlists (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        name       TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );

    CREATE TABLE playlist_tracks (
        playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
        track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        position    INTEGER NOT NULL,
        PRIMARY KEY (playlist_id, position)
    );

    CREATE TABLE mix_history (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        from_track INTEGER REFERENCES tracks(id) ON DELETE SET NULL,
        to_track   INTEGER REFERENCES tracks(id) ON DELETE SET NULL,
        event      TEXT NOT NULL,
        at         INTEGER NOT NULL
    );

    CREATE TABLE analysis_jobs (
        id               INTEGER PRIMARY KEY AUTOINCREMENT,
        track_id         INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        kind             TEXT NOT NULL,
        status           TEXT NOT NULL DEFAULT 'queued',
        analysis_version INTEGER NOT NULL,
        created_at       INTEGER NOT NULL,
        updated_at       INTEGER NOT NULL,
        error            TEXT
    );
    CREATE INDEX idx_jobs_status ON analysis_jobs(status);
    "#,
    // --- v2: detected song-structure sections (spec §7) ---------------------
    r#"
    CREATE TABLE sections (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        track_id   INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        start_ms   INTEGER NOT NULL,
        end_ms     INTEGER NOT NULL,
        kind       TEXT NOT NULL,
        confidence REAL NOT NULL DEFAULT 0.0
    );
    CREATE INDEX idx_sections_track ON sections(track_id);
    "#,
];

/// Applies any pending migrations to bring `conn` up to [`SCHEMA_VERSION`].
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (idx, ddl) in MIGRATIONS.iter().enumerate() {
        let version = idx as i64 + 1;
        if version > current {
            conn.execute_batch(ddl)?;
            // user_version does not accept bound params; the value is trusted.
            conn.execute_batch(&format!("PRAGMA user_version = {version};"))?;
        }
    }
    Ok(())
}
