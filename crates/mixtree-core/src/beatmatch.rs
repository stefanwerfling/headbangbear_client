//! Beatmatching helpers (spec §5: Beatmatching / Tempo-Anpassung).
//!
//! Pure tempo math: compute the playback-rate multiplier ("pitch") that makes a
//! deck's tempo match a master tempo, accounting for half-/double-time so a
//! 140-BPM track syncs to a 70-BPM master at rate `1.0` (via the 70↔140 octave)
//! rather than an extreme `0.5`.

/// The playback-rate multiplier to apply to a `deck_bpm` track so its *effective*
/// tempo matches `master_bpm`, picking the nearest half/double octave.
///
/// Returns `1.0` when either tempo is unknown/zero. The result is clamped to the
/// engine's supported range `0.5..=2.0`.
pub fn sync_pitch(master_bpm: f64, deck_bpm: f64) -> f64 {
    if master_bpm <= 0.0 || deck_bpm <= 0.0 {
        return 1.0;
    }
    // The three octave-equivalent ratios; pick the one closest to 1.0 (smallest
    // pitch change), then clamp to the engine's supported range.
    let base = master_bpm / deck_bpm;
    let candidates = [base, base * 0.5, base * 2.0];
    let best = candidates
        .into_iter()
        .min_by(|a, b| {
            (a - 1.0)
                .abs()
                .partial_cmp(&(b - 1.0).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(1.0);
    best.clamp(0.5, 2.0)
}

/// The effective tempo of a `deck_bpm` track played at `pitch`.
pub fn effective_bpm(deck_bpm: f64, pitch: f64) -> f64 {
    deck_bpm * pitch
}

/// Percentage pitch adjustment for display, e.g. `pitch = 1.06` → `+6.0`.
pub fn pitch_percent(pitch: f64) -> f64 {
    (pitch - 1.0) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn equal_tempo_is_unity() {
        assert!(close(sync_pitch(128.0, 128.0), 1.0));
    }

    #[test]
    fn matches_within_octave() {
        // 128 master, 124 deck → speed up ~3.2 %.
        let p = sync_pitch(128.0, 124.0);
        assert!(close(effective_bpm(124.0, p), 128.0));
        assert!(p > 1.0 && p < 1.05);
    }

    #[test]
    fn octave_aligned_tempos_need_no_pitch_change() {
        // 140 vs 70 are already octave-synced → minimal change is pitch 1.0,
        // the beats align (every other beat). Same for 70 vs 140.
        assert!(close(sync_pitch(140.0, 70.0), 1.0));
        assert!(close(sync_pitch(70.0, 140.0), 1.0));
    }

    #[test]
    fn picks_octave_minimizing_pitch_change() {
        // Master 128, deck 65 (~half of 128): doubling 65→130 (pitch ~0.985) is a
        // far smaller change than playing 65 as-is (pitch ~1.97).
        let p = sync_pitch(128.0, 65.0);
        assert!((p - 128.0 / 130.0).abs() < 1e-6, "p = {p}");
        assert!(p > 0.9 && p < 1.1);
    }

    #[test]
    fn clamps_extreme_ratios() {
        // Master 200, deck 40: nearest octave of 40 to 200 is 80 (40*2), ratio
        // 2.5 → clamped to 2.0.
        let p = sync_pitch(200.0, 40.0);
        assert!((0.5..=2.0).contains(&p));
    }

    #[test]
    fn unknown_tempo_is_unity() {
        assert!(close(sync_pitch(0.0, 128.0), 1.0));
        assert!(close(sync_pitch(128.0, 0.0), 1.0));
    }

    #[test]
    fn pitch_percent_display() {
        assert!((pitch_percent(1.06) - 6.0).abs() < 1e-6);
        assert!((pitch_percent(0.94) + 6.0).abs() < 1e-6);
    }
}
