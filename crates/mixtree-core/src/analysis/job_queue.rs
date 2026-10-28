//! Persisted background analysis job queue (spec §8: "große Analysejobs in
//! einer Hintergrundwarteschlange").
//!
//! Jobs are rows in `analysis_jobs`. Phase 2 will pop queued jobs on a worker
//! thread, decode the audio and run the detectors from [`super`]; Phase 1 only
//! needs the durable queue so the import flow can enqueue work.

use crate::analysis::AnalysisKind;
use crate::database::MixtreeDb;
use crate::error::Result;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Lifecycle status of an analysis job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Waiting to be processed.
    Queued,
    /// Currently being processed.
    Running,
    /// Finished successfully.
    Done,
    /// Failed; see [`AnalysisJob::error`].
    Failed,
}

impl JobStatus {
    fn as_str(self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Done => "done",
            JobStatus::Failed => "failed",
        }
    }
    fn from_str(s: &str) -> Self {
        match s {
            "running" => JobStatus::Running,
            "done" => JobStatus::Done,
            "failed" => JobStatus::Failed,
            _ => JobStatus::Queued,
        }
    }
}

/// A single analysis job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisJob {
    /// Job id.
    pub id: i64,
    /// Target track.
    pub track_id: i64,
    /// What to compute.
    pub kind: AnalysisKind,
    /// Current status.
    pub status: JobStatus,
    /// Analysis algorithm version this job targets.
    pub analysis_version: i32,
    /// Error message when `status == Failed`.
    pub error: Option<String>,
}

impl MixtreeDb {
    /// Enqueues an analysis job and returns its id.
    pub fn enqueue_job(
        &self,
        track_id: i64,
        kind: AnalysisKind,
        analysis_version: i32,
        now: i64,
    ) -> Result<i64> {
        self.conn().execute(
            "INSERT INTO analysis_jobs(track_id, kind, status, analysis_version, created_at, updated_at)
             VALUES(?1, ?2, 'queued', ?3, ?4, ?4)",
            params![track_id, kind.as_str(), analysis_version, now],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Claims the oldest queued job, marking it `running`. Returns `None` when
    /// the queue is empty.
    pub fn claim_next_job(&self, now: i64) -> Result<Option<AnalysisJob>> {
        let job = self
            .conn()
            .query_row(
                "SELECT id, track_id, kind, status, analysis_version, error
                 FROM analysis_jobs WHERE status = 'queued'
                 ORDER BY created_at, id LIMIT 1",
                [],
                map_job,
            )
            .optional()?;
        if let Some(job) = &job {
            self.conn().execute(
                "UPDATE analysis_jobs SET status='running', updated_at=?2 WHERE id=?1",
                params![job.id, now],
            )?;
        }
        Ok(job.map(|mut j| {
            j.status = JobStatus::Running;
            j
        }))
    }

    /// Marks a job finished (`done` or `failed`).
    pub fn finish_job(
        &self,
        id: i64,
        status: JobStatus,
        error: Option<&str>,
        now: i64,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE analysis_jobs SET status=?2, error=?3, updated_at=?4 WHERE id=?1",
            params![id, status.as_str(), error, now],
        )?;
        Ok(())
    }

    /// Counts jobs by status (for the UI's background-work indicator).
    pub fn count_jobs(&self, status: JobStatus) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM analysis_jobs WHERE status = ?1",
            [status.as_str()],
            |r| r.get(0),
        )?)
    }
}

fn map_job(r: &rusqlite::Row) -> rusqlite::Result<AnalysisJob> {
    Ok(AnalysisJob {
        id: r.get(0)?,
        track_id: r.get(1)?,
        kind: AnalysisKind::from_str(&r.get::<_, String>(2)?).unwrap_or(AnalysisKind::Bpm),
        status: JobStatus::from_str(&r.get::<_, String>(3)?),
        analysis_version: r.get(4)?,
        error: r.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Track;

    fn db_with_track() -> (MixtreeDb, i64) {
        let db = MixtreeDb::open_in_memory().unwrap();
        let id = db
            .insert_track(&Track::new_from_path("/m/a.mp3", "A", 0))
            .unwrap();
        (db, id)
    }

    #[test]
    fn enqueue_claim_finish_flow() {
        let (db, track) = db_with_track();
        db.enqueue_job(track, AnalysisKind::Bpm, 1, 100).unwrap();
        db.enqueue_job(track, AnalysisKind::Key, 1, 101).unwrap();
        assert_eq!(db.count_jobs(JobStatus::Queued).unwrap(), 2);

        let job = db.claim_next_job(200).unwrap().unwrap();
        assert_eq!(job.kind, AnalysisKind::Bpm); // oldest first
        assert_eq!(job.status, JobStatus::Running);
        assert_eq!(db.count_jobs(JobStatus::Queued).unwrap(), 1);

        db.finish_job(job.id, JobStatus::Done, None, 300).unwrap();
        assert_eq!(db.count_jobs(JobStatus::Done).unwrap(), 1);
    }

    #[test]
    fn empty_queue_claims_nothing() {
        let db = MixtreeDb::open_in_memory().unwrap();
        assert!(db.claim_next_job(1).unwrap().is_none());
    }

    #[test]
    fn failed_job_records_error() {
        let (db, track) = db_with_track();
        let id = db.enqueue_job(track, AnalysisKind::Energy, 1, 1).unwrap();
        let job = db.claim_next_job(2).unwrap().unwrap();
        assert_eq!(job.id, id);
        db.finish_job(id, JobStatus::Failed, Some("decode error"), 3)
            .unwrap();
        assert_eq!(db.count_jobs(JobStatus::Failed).unwrap(), 1);
    }
}
