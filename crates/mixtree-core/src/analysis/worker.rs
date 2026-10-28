//! The analysis worker: drains the job queue, decodes audio once per track, runs
//! the detectors and writes **versioned** results back to the library (spec §7, §8).
//!
//! The worker is backend-agnostic: it takes a `decode` closure so the Tauri shell
//! can plug in [`decode::decode_for_analysis`](super::decode::decode_for_analysis)
//! while tests inject synthetic audio. Results carry the current
//! [`ANALYSIS_VERSION`](crate::database::schema::ANALYSIS_VERSION); re-running with
//! a newer algorithm version re-analyzes automatically.

use crate::analysis::bpm::AutocorrelationBpmDetector;
use crate::analysis::decode::DecodedAudio;
use crate::analysis::energy::SpectralEnergyAnalyzer;
use crate::analysis::job_queue::JobStatus;
use crate::analysis::key_detection::ChromaKeyDetector;
use crate::analysis::{AnalysisKind, BpmDetector, EnergyAnalyzer, KeyDetector};
use crate::database::{schema, MixtreeDb};
use crate::error::{CoreError, Result};
use crate::models::AnalysisState;
use std::collections::HashMap;
use std::path::Path;

/// The set of detectors used by the worker.
pub struct AnalysisEngine {
    /// Tempo / beat-grid detector.
    pub bpm: Box<dyn BpmDetector + Send + Sync>,
    /// Key detector.
    pub key: Box<dyn KeyDetector + Send + Sync>,
    /// Energy estimator.
    pub energy: Box<dyn EnergyAnalyzer + Send + Sync>,
}

impl Default for AnalysisEngine {
    fn default() -> Self {
        AnalysisEngine {
            bpm: Box::new(AutocorrelationBpmDetector),
            key: Box::new(ChromaKeyDetector),
            energy: Box::new(SpectralEnergyAnalyzer),
        }
    }
}

/// Outcome of a single [`run_pending`] pass.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnalysisPassReport {
    /// Jobs processed in this pass.
    pub processed: usize,
    /// Jobs that completed successfully.
    pub succeeded: usize,
    /// Jobs that failed.
    pub failed: usize,
}

/// Processes up to `max_jobs` queued jobs, decoding each track at most once.
///
/// `now` is the timestamp used for job/update bookkeeping. `decode` turns a file
/// path into analysis-ready mono audio.
pub fn run_pending<F>(
    db: &MixtreeDb,
    engine: &AnalysisEngine,
    max_jobs: usize,
    now: i64,
    decode: F,
) -> Result<AnalysisPassReport>
where
    F: Fn(&Path) -> Result<DecodedAudio>,
{
    let mut report = AnalysisPassReport::default();
    // Cache decoded audio within the pass so a track with bpm+key+energy jobs is
    // decoded only once.
    let mut cache: HashMap<i64, std::result::Result<DecodedAudio, String>> = HashMap::new();

    for _ in 0..max_jobs {
        let job = match db.claim_next_job(now)? {
            Some(j) => j,
            None => break,
        };
        report.processed += 1;

        let target = JobTarget {
            track_id: job.track_id,
            kind: job.kind,
        };
        match process_job(db, engine, &target, &mut cache, &decode, now) {
            Ok(()) => {
                db.finish_job(job.id, JobStatus::Done, None, now)?;
                report.succeeded += 1;
            }
            Err(e) => {
                let msg = e.to_string();
                db.finish_job(job.id, JobStatus::Failed, Some(&msg), now)?;
                mark_track_failed(db, job.track_id, now)?;
                report.failed += 1;
            }
        }
    }
    Ok(report)
}

/// Lightweight (track_id, kind) view of a job so `process_job` doesn't need the
/// whole queue row.
struct JobTarget {
    track_id: i64,
    kind: AnalysisKind,
}

fn process_job<F>(
    db: &MixtreeDb,
    engine: &AnalysisEngine,
    target: &JobTarget,
    cache: &mut HashMap<i64, std::result::Result<DecodedAudio, String>>,
    decode: &F,
    now: i64,
) -> Result<()>
where
    F: Fn(&Path) -> Result<DecodedAudio>,
{
    let mut track = db
        .get_track(target.track_id)?
        .ok_or_else(|| CoreError::NotFound(format!("track {}", target.track_id)))?;

    // Decode once per track per pass (cached, including the failure case).
    let audio = cache
        .entry(target.track_id)
        .or_insert_with(|| decode(Path::new(&track.file_path)).map_err(|e| e.to_string()));
    let audio = match audio {
        Ok(a) => a,
        Err(e) => {
            return Err(CoreError::Metadata {
                path: track.file_path.clone(),
                message: e.clone(),
            })
        }
    };

    match target.kind {
        AnalysisKind::Bpm => {
            let r = engine
                .bpm
                .detect(&audio.samples, audio.sample_rate)
                .ok_or_else(|| CoreError::Invalid("BPM detection produced no result".into()))?;
            track.bpm = Some(r.bpm);
        }
        AnalysisKind::Key => {
            let r = engine
                .key
                .detect(&audio.samples, audio.sample_rate)
                .ok_or_else(|| CoreError::Invalid("key detection produced no result".into()))?;
            track.key = Some(r.key);
        }
        AnalysisKind::Energy => {
            let r = engine
                .energy
                .analyze(&audio.samples, audio.sample_rate)
                .ok_or_else(|| CoreError::Invalid("energy analysis produced no result".into()))?;
            track.energy = Some(r.energy);
        }
        AnalysisKind::Structure => {
            let sections =
                crate::structure_detection::detect_structure(&audio.samples, audio.sample_rate);
            db.replace_sections(track.id, &sections)?;
            // Refresh suggested (structural) cues, preserving manual hot-cues.
            db.clear_structural_cues(track.id)?;
            for cue in crate::structure_detection::suggest_cues(&sections) {
                db.add_cue(track.id, &cue)?;
            }
        }
    }

    track.analysis_version = schema::ANALYSIS_VERSION;
    track.analysis_state = AnalysisState::Done;
    let _ = now;
    db.update_track(&track)?;
    Ok(())
}

fn mark_track_failed(db: &MixtreeDb, track_id: i64, _now: i64) -> Result<()> {
    if let Some(mut t) = db.get_track(track_id)? {
        t.analysis_state = AnalysisState::Failed;
        db.update_track(&t)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::decode::ANALYSIS_SAMPLE_RATE;
    use crate::database::schema::ANALYSIS_VERSION;
    use crate::models::Track;

    /// A synthetic decoded click train at a known BPM.
    fn synthetic(bpm: f64) -> DecodedAudio {
        let sr = ANALYSIS_SAMPLE_RATE;
        let seconds = 12.0;
        let n = (sr as f64 * seconds) as usize;
        let mut samples = vec![0.0f32; n];
        let period = (60.0 / bpm * sr as f64) as usize;
        let mut i = 0;
        while i < n {
            for j in 0..48 {
                if i + j < n {
                    samples[i + j] += (1.0 - j as f32 / 48.0) * ((j as f32) * 0.6).sin();
                }
            }
            i += period;
        }
        DecodedAudio {
            samples,
            sample_rate: sr,
        }
    }

    #[test]
    fn worker_fills_bpm_key_energy_and_drains_queue() {
        let db = MixtreeDb::open_in_memory().unwrap();
        let mut t = Track::new_from_path("/music/track.flac", "T", 0);
        t.analysis_state = AnalysisState::Pending;
        let id = db.insert_track(&t).unwrap();
        db.enqueue_job(id, AnalysisKind::Bpm, ANALYSIS_VERSION, 1)
            .unwrap();
        db.enqueue_job(id, AnalysisKind::Key, ANALYSIS_VERSION, 1)
            .unwrap();
        db.enqueue_job(id, AnalysisKind::Energy, ANALYSIS_VERSION, 1)
            .unwrap();

        let engine = AnalysisEngine::default();
        // Inject synthetic audio; also count how often decode runs.
        let calls = std::cell::Cell::new(0);
        let report = run_pending(&db, &engine, 10, 2, |_p| {
            calls.set(calls.get() + 1);
            Ok(synthetic(128.0))
        })
        .unwrap();

        assert_eq!(report.processed, 3);
        assert_eq!(report.succeeded, 3);
        assert_eq!(calls.get(), 1, "track decoded only once for all three jobs");

        let got = db.get_track(id).unwrap().unwrap();
        assert!(got.bpm.is_some(), "bpm written");
        let bpm = got.bpm.unwrap();
        assert!(
            (bpm - 128.0).abs() < 4.0 || (bpm - 64.0).abs() < 4.0 || (bpm - 256.0).abs() < 4.0,
            "bpm {bpm}"
        );
        assert!(got.key.is_some(), "key written");
        assert!(got.energy.is_some(), "energy written");
        assert_eq!(got.analysis_state, AnalysisState::Done);
        assert_eq!(got.analysis_version, ANALYSIS_VERSION);
        assert_eq!(db.count_jobs(JobStatus::Done).unwrap(), 3);
    }

    #[test]
    fn decode_failure_marks_job_and_track_failed() {
        let db = MixtreeDb::open_in_memory().unwrap();
        let id = db
            .insert_track(&Track::new_from_path("/music/missing.mp3", "M", 0))
            .unwrap();
        db.enqueue_job(id, AnalysisKind::Bpm, ANALYSIS_VERSION, 1)
            .unwrap();

        let report = run_pending(&db, &AnalysisEngine::default(), 10, 2, |p| {
            Err(CoreError::Metadata {
                path: p.display().to_string(),
                message: "no file".into(),
            })
        })
        .unwrap();

        assert_eq!(report.failed, 1);
        assert_eq!(db.count_jobs(JobStatus::Failed).unwrap(), 1);
        assert_eq!(
            db.get_track(id).unwrap().unwrap().analysis_state,
            AnalysisState::Failed
        );
    }

    #[test]
    fn empty_queue_is_a_noop() {
        let db = MixtreeDb::open_in_memory().unwrap();
        let report = run_pending(&db, &AnalysisEngine::default(), 10, 1, |_| {
            Ok(DecodedAudio {
                samples: vec![],
                sample_rate: 11_025,
            })
        })
        .unwrap();
        assert_eq!(report, AnalysisPassReport::default());
    }
}
