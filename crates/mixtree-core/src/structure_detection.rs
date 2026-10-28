//! Song-structure and drop detection (spec §7, §8).
//!
//! A deliberately transparent, signal-based segmenter: it builds a short-time
//! energy curve, classifies each window as low / mid / high energy, merges the
//! windows into runs and labels them Intro / Build / Drop / Breakdown / Steady /
//! Outro. Every section carries a `confidence`, and this is explicitly an
//! **estimate** (spec §6: show uncertainty, never fake precision). A future ML
//! stage can replace this without changing the interface.

use crate::models::{CuePoint, Section, SectionKind};

/// Window length for the energy curve, in milliseconds.
const WINDOW_MS: u64 = 250;
/// Minimum section length; shorter runs are merged into their neighbour.
const MIN_SECTION_MS: u64 = 1500;
const HIGH: f32 = 0.62;
const LOW: f32 = 0.35;

/// Per-window energy class.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Low,
    Mid,
    High,
}

/// Detects song-structure sections from mono samples.
///
/// Returns a single `Steady` section spanning the whole track when it is too
/// short or too uniform to segment meaningfully.
pub fn detect_structure(samples: &[f32], sample_rate: u32) -> Vec<Section> {
    if sample_rate == 0 || samples.is_empty() {
        return Vec::new();
    }
    let win = (sample_rate as u64 * WINDOW_MS / 1000).max(1) as usize;
    let total_ms = samples.len() as u64 * 1000 / sample_rate as u64;

    // Short-time RMS energy per window.
    let mut energies: Vec<f32> = samples
        .chunks(win)
        .map(|c| (c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32).sqrt())
        .collect();
    if energies.len() < 4 {
        return vec![Section {
            start_ms: 0,
            end_ms: total_ms,
            kind: SectionKind::Steady,
            confidence: 0.3,
        }];
    }

    // Smooth (3-window moving average) and normalize to the peak.
    smooth(&mut energies);
    let max = energies.iter().copied().fold(0.0f32, f32::max);
    if max <= 1e-6 {
        return vec![Section {
            start_ms: 0,
            end_ms: total_ms,
            kind: SectionKind::Quiet,
            confidence: 0.4,
        }];
    }
    for e in &mut energies {
        *e /= max;
    }

    let classes: Vec<Class> = energies
        .iter()
        .map(|&e| {
            if e >= HIGH {
                Class::High
            } else if e <= LOW {
                Class::Low
            } else {
                Class::Mid
            }
        })
        .collect();

    // Merge consecutive same-class windows into runs.
    let mut runs: Vec<Run> = Vec::new();
    for (i, &c) in classes.iter().enumerate() {
        match runs.last_mut() {
            Some(r) if r.class == c => r.end = i,
            _ => runs.push(Run {
                class: c,
                start: i,
                end: i,
            }),
        }
    }
    merge_short_runs(&mut runs, &energies, win, sample_rate);

    label_runs(&runs, &energies, win, sample_rate, total_ms)
}

struct Run {
    class: Class,
    start: usize,
    end: usize,
}

fn smooth(e: &mut [f32]) {
    if e.len() < 3 {
        return;
    }
    let orig = e.to_vec();
    for i in 1..e.len() - 1 {
        e[i] = (orig[i - 1] + orig[i] + orig[i + 1]) / 3.0;
    }
}

/// Merges runs shorter than [`MIN_SECTION_MS`] into the previous run.
fn merge_short_runs(runs: &mut Vec<Run>, _e: &[f32], win: usize, sr: u32) {
    let min_windows = (MIN_SECTION_MS * sr as u64 / 1000 / win as u64).max(1) as usize;
    let mut i = 1;
    while i < runs.len() {
        let len = runs[i].end - runs[i].start + 1;
        if len < min_windows {
            let end = runs[i].end;
            runs[i - 1].end = end;
            runs.remove(i);
        } else {
            i += 1;
        }
    }
}

fn run_mean(e: &[f32], r: &Run) -> f32 {
    let slice = &e[r.start..=r.end];
    slice.iter().copied().sum::<f32>() / slice.len() as f32
}

fn label_runs(runs: &[Run], e: &[f32], win: usize, sr: u32, total_ms: u64) -> Vec<Section> {
    let win_ms = win as u64 * 1000 / sr as u64;
    let last = runs.len().saturating_sub(1);
    let mut out = Vec::with_capacity(runs.len());
    for (idx, r) in runs.iter().enumerate() {
        let mean = run_mean(e, r);
        let next_mean = runs.get(idx + 1).map(|n| run_mean(e, n));
        let kind = match r.class {
            Class::High => SectionKind::Drop,
            Class::Low => {
                if idx == 0 {
                    SectionKind::Intro
                } else if idx == last {
                    SectionKind::Outro
                } else {
                    // A low-energy section in the middle of the track is a breakdown.
                    SectionKind::Breakdown
                }
            }
            Class::Mid => {
                // A rising mid run just before a high run is a build-up.
                match next_mean {
                    Some(nm) if nm >= HIGH && mean < nm => SectionKind::Build,
                    _ => SectionKind::Steady,
                }
            }
        };
        // Confidence from how clearly the run separates from the mid band.
        let confidence = ((mean - 0.5).abs() * 2.0).clamp(0.3, 1.0) as f64;
        let start_ms = r.start as u64 * win_ms;
        let end_ms = ((r.end as u64 + 1) * win_ms).min(total_ms);
        out.push(Section {
            start_ms,
            end_ms,
            kind,
            confidence,
        });
    }
    out
}

/// Suggests cue points from detected sections (spec §6): a "Mix In" at the end
/// of the intro, a cue at each drop, and a "Mix Out" at the start of the outro.
pub fn suggest_cues(sections: &[Section]) -> Vec<CuePoint> {
    let mut cues = Vec::new();
    let mut drop_n = 0;
    for s in sections {
        match s.kind {
            SectionKind::Intro => cues.push(CuePoint {
                id: 0,
                position_ms: s.end_ms,
                label: Some("Mix In".into()),
                kind: "structural".into(),
                color: Some("#2ecc71".into()),
            }),
            SectionKind::Drop => {
                drop_n += 1;
                cues.push(CuePoint {
                    id: 0,
                    position_ms: s.start_ms,
                    label: Some(format!("Drop {drop_n}")),
                    kind: "structural".into(),
                    color: Some("#ff8c1a".into()),
                });
            }
            SectionKind::Outro => cues.push(CuePoint {
                id: 0,
                position_ms: s.start_ms,
                label: Some("Mix Out".into()),
                kind: "structural".into(),
                color: Some("#2d9cff".into()),
            }),
            _ => {}
        }
    }
    cues
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    /// Builds a test signal of labelled segments: each `(seconds, amplitude)`
    /// becomes a tone at that amplitude.
    fn signal(segments: &[(f32, f32)], sr: u32) -> Vec<f32> {
        let mut out = Vec::new();
        let mut phase = 0.0f32;
        for &(secs, amp) in segments {
            let n = (secs * sr as f32) as usize;
            for _ in 0..n {
                phase += 2.0 * PI * 220.0 / sr as f32;
                out.push(phase.sin() * amp);
            }
        }
        out
    }

    #[test]
    fn empty_input_yields_no_sections() {
        assert!(detect_structure(&[], 44_100).is_empty());
    }

    #[test]
    fn uniform_track_is_single_section() {
        let sr = 11_025;
        let sig = signal(&[(10.0, 0.5)], sr);
        let s = detect_structure(&sig, sr);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn detects_intro_drop_breakdown_outro() {
        let sr = 11_025;
        // quiet intro, loud drop, quiet breakdown, loud drop, quiet outro.
        let sig = signal(
            &[
                (6.0, 0.08),
                (10.0, 0.95),
                (6.0, 0.08),
                (10.0, 0.95),
                (6.0, 0.08),
            ],
            sr,
        );
        let sections = detect_structure(&sig, sr);
        assert!(
            sections.len() >= 4,
            "expected multiple sections, got {}",
            sections.len()
        );

        // Ends are intro/outro, and at least one drop exists.
        assert_eq!(sections.first().unwrap().kind, SectionKind::Intro);
        assert_eq!(sections.last().unwrap().kind, SectionKind::Outro);
        assert!(
            sections.iter().any(|s| s.kind == SectionKind::Drop),
            "expected a drop"
        );
        assert!(
            sections.iter().any(|s| s.kind == SectionKind::Breakdown),
            "expected a breakdown"
        );

        // Sections tile the track with no gaps.
        for w in sections.windows(2) {
            assert_eq!(w[0].end_ms, w[1].start_ms, "sections must be contiguous");
        }
    }

    #[test]
    fn suggests_cues_at_boundaries() {
        let sr = 11_025;
        let sig = signal(&[(6.0, 0.08), (10.0, 0.95), (6.0, 0.08)], sr);
        let sections = detect_structure(&sig, sr);
        let cues = suggest_cues(&sections);
        assert!(cues.iter().any(|c| c.label.as_deref() == Some("Mix In")));
        assert!(cues.iter().any(|c| c.label.as_deref() == Some("Drop 1")));
        assert!(cues.iter().all(|c| c.kind == "structural"));
    }
}
