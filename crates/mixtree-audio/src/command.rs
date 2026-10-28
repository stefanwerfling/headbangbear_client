//! Control messages sent from the UI thread to the audio thread, plus the
//! immutable audio payload a deck plays.

use std::sync::Arc;

/// Decoded audio ready for playback: interleaved `f32` samples.
#[derive(Clone)]
pub struct TrackAudio {
    /// Interleaved samples (`channels` per frame).
    pub samples: Arc<[f32]>,
    /// Channel count (1 or 2 supported; >2 uses the first two).
    pub channels: u16,
    /// Source sample rate in Hz.
    pub sample_rate: u32,
}

impl TrackAudio {
    /// Number of frames (samples per channel).
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / self.channels as usize
        }
    }

    /// Total duration in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            0
        } else {
            (self.frames() as u64 * 1000) / self.sample_rate as u64
        }
    }

    /// Computes a normalized waveform envelope: the peak (max absolute, mono-summed)
    /// amplitude of each of `buckets` equal time slices, scaled so the loudest
    /// bucket is `1.0`. Returns an empty vector for empty audio.
    pub fn peaks(&self, buckets: usize) -> Vec<f32> {
        let frames = self.frames();
        if frames == 0 || buckets == 0 {
            return Vec::new();
        }
        let ch = self.channels.max(1) as usize;
        let per = frames.div_ceil(buckets).max(1);
        let mut out = Vec::with_capacity(buckets);
        let mut max = 0.0f32;
        for b in 0..buckets {
            let start = b * per;
            if start >= frames {
                break;
            }
            let end = (start + per).min(frames);
            let mut peak = 0.0f32;
            for f in start..end {
                let mut s = 0.0f32;
                for c in 0..ch {
                    s += self.samples[f * ch + c].abs();
                }
                peak = peak.max(s / ch as f32);
            }
            max = max.max(peak);
            out.push(peak);
        }
        if max > 0.0 {
            for v in &mut out {
                *v /= max;
            }
        }
        out
    }
}

/// A control command for a deck or the mixer.
pub enum Command {
    /// Load (or clear with `None`) a deck's audio. Resets position to the cue.
    Load {
        deck: usize,
        audio: Option<TrackAudio>,
    },
    /// Start playback.
    Play(usize),
    /// Pause playback (keeps position).
    Pause(usize),
    /// Toggle play/pause.
    TogglePlay(usize),
    /// Jump to the stored cue point (and keep playing state).
    Cue(usize),
    /// Set the cue point to the current position.
    SetCue(usize),
    /// Seek to an absolute position in milliseconds.
    Seek { deck: usize, ms: u64 },
    /// Set the deck's linear output gain (`0.0..=1.0+`).
    SetGain { deck: usize, gain: f32 },
    /// Set the deck's playback-rate multiplier (pitch/tempo; `1.0` = original).
    SetPitch { deck: usize, pitch: f64 },
    /// Set the 3-band EQ controls `[low, mid, high]`, each `0.0..=1.0`.
    SetEq { deck: usize, gains: [f32; 3] },
    /// Enable a loop between two absolute positions (ms), or disable with `None`.
    SetLoop {
        deck: usize,
        range_ms: Option<(u64, u64)>,
    },
    /// Equal-power crossfader position: `0.0` = deck A, `1.0` = deck B.
    SetCrossfader(f32),
    /// Automated crossfade to `target` over `duration_ms` (spec §6: geplanter
    /// Übergang). Replaces any crossfade in progress.
    Crossfade { target: f32, duration_ms: u64 },
    /// Master output gain.
    SetMaster(f32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_are_normalized_and_sized() {
        // A mono ramp from 0 to 1 over 1000 frames.
        let samples: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
        let audio = TrackAudio {
            samples: Arc::from(samples.into_boxed_slice()),
            channels: 1,
            sample_rate: 44_100,
        };
        let p = audio.peaks(10);
        assert_eq!(p.len(), 10);
        // Loudest bucket (the end of the ramp) is normalized to 1.0, and buckets
        // increase monotonically for a ramp.
        assert!((p[9] - 1.0).abs() < 1e-6);
        assert!(p[0] < p[9]);
        assert!(p.iter().all(|&v| (0.0..=1.0).contains(&v)));
    }

    #[test]
    fn peaks_empty_audio() {
        let audio = TrackAudio {
            samples: Arc::from(Vec::new().into_boxed_slice()),
            channels: 2,
            sample_rate: 44_100,
        };
        assert!(audio.peaks(10).is_empty());
    }
}
