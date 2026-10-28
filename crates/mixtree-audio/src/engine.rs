//! The real-time mixing engine: two decks → equal-power crossfader → master.
//!
//! [`Engine::render`] is allocation-free and does only arithmetic + filtering,
//! so it is safe to call from a real-time audio callback (spec §5). All control
//! happens via [`Command`]s applied with [`Engine::apply`].

use crate::command::{Command, TrackAudio};
use crate::eq::Eq3;

const NUM_DECKS: usize = 2;

/// One playback deck.
struct Deck {
    audio: Option<TrackAudio>,
    /// Playhead in source frames (fractional for resampling).
    pos: f64,
    playing: bool,
    gain: f32,
    /// Playback-rate multiplier (pitch/tempo).
    pitch: f64,
    cue: f64,
    loop_range: Option<(f64, f64)>,
    eq_l: Eq3,
    eq_r: Eq3,
    eq_gains: [f32; 3],
}

impl Deck {
    fn new(fs: f32) -> Self {
        let mut d = Deck {
            audio: None,
            pos: 0.0,
            playing: false,
            gain: 1.0,
            pitch: 1.0,
            cue: 0.0,
            loop_range: None,
            eq_l: Eq3::default(),
            eq_r: Eq3::default(),
            eq_gains: [0.5, 0.5, 0.5],
        };
        d.eq_l.configure(fs, d.eq_gains);
        d.eq_r.configure(fs, d.eq_gains);
        d
    }

    fn frames(&self) -> f64 {
        self.audio
            .as_ref()
            .map(|a| a.frames() as f64)
            .unwrap_or(0.0)
    }

    fn src_rate(&self) -> f64 {
        self.audio
            .as_ref()
            .map(|a| a.sample_rate as f64)
            .unwrap_or(44_100.0)
    }

    /// Reads an interpolated stereo frame `[l, r]` at the current position.
    fn read_stereo(&self) -> [f32; 2] {
        let Some(a) = &self.audio else {
            return [0.0, 0.0];
        };
        let ch = a.channels.max(1) as usize;
        let frames = a.frames();
        if frames == 0 {
            return [0.0, 0.0];
        }
        let i0 = self.pos.floor() as usize;
        let frac = (self.pos - i0 as f64) as f32;
        let i1 = (i0 + 1).min(frames - 1);
        let get = |frame: usize, c: usize| -> f32 {
            let c = if ch == 1 { 0 } else { c.min(ch - 1) };
            a.samples[frame * ch + c]
        };
        let l = get(i0, 0) * (1.0 - frac) + get(i1, 0) * frac;
        let r = get(i0, 1) * (1.0 - frac) + get(i1, 1) * frac;
        [l, r]
    }
}

/// An in-progress automated crossfade.
struct CrossfadeRamp {
    target: f32,
    per_frame: f32,
    remaining: u64,
}

/// The mixing engine.
pub struct Engine {
    decks: [Deck; NUM_DECKS],
    crossfader: f32,
    crossfade: Option<CrossfadeRamp>,
    master: f32,
    out_rate: u32,
    out_channels: u16,
}

impl Engine {
    /// Creates an engine for the given output format.
    pub fn new(out_rate: u32, out_channels: u16) -> Self {
        let fs = out_rate as f32;
        Engine {
            decks: [Deck::new(fs), Deck::new(fs)],
            crossfader: 0.5,
            crossfade: None,
            master: 0.9,
            out_rate,
            out_channels: out_channels.max(1),
        }
    }

    /// Output sample rate.
    pub fn out_rate(&self) -> u32 {
        self.out_rate
    }

    /// Current crossfader position (`0.0` = A, `1.0` = B).
    pub fn crossfader(&self) -> f32 {
        self.crossfader
    }

    /// Whether a deck is currently playing.
    pub fn is_playing(&self, deck: usize) -> bool {
        self.decks.get(deck).map(|d| d.playing).unwrap_or(false)
    }

    /// Current playhead of a deck in milliseconds.
    pub fn position_ms(&self, deck: usize) -> u64 {
        match self.decks.get(deck) {
            Some(d) if d.src_rate() > 0.0 => ((d.pos / d.src_rate()) * 1000.0) as u64,
            _ => 0,
        }
    }

    fn ms_to_frames(deck: &Deck, ms: u64) -> f64 {
        ms as f64 / 1000.0 * deck.src_rate()
    }

    /// Applies a control command.
    pub fn apply(&mut self, cmd: Command) {
        match cmd {
            Command::Load { deck, audio } => {
                if let Some(d) = self.decks.get_mut(deck) {
                    d.audio = audio;
                    d.pos = 0.0;
                    d.cue = 0.0;
                    d.playing = false;
                    d.loop_range = None;
                    d.eq_l.reset();
                    d.eq_r.reset();
                }
            }
            Command::Play(deck) => self.set_playing(deck, true),
            Command::Pause(deck) => self.set_playing(deck, false),
            Command::TogglePlay(deck) => {
                if let Some(d) = self.decks.get_mut(deck) {
                    d.playing = !d.playing && d.audio.is_some();
                }
            }
            Command::Cue(deck) => {
                if let Some(d) = self.decks.get_mut(deck) {
                    d.pos = d.cue;
                }
            }
            Command::SetCue(deck) => {
                if let Some(d) = self.decks.get_mut(deck) {
                    d.cue = d.pos;
                }
            }
            Command::Seek { deck, ms } => {
                if let Some(d) = self.decks.get_mut(deck) {
                    d.pos = Self::ms_to_frames(d, ms).min(d.frames());
                }
            }
            Command::SetGain { deck, gain } => {
                if let Some(d) = self.decks.get_mut(deck) {
                    d.gain = gain.max(0.0);
                }
            }
            Command::SetPitch { deck, pitch } => {
                if let Some(d) = self.decks.get_mut(deck) {
                    d.pitch = pitch.clamp(0.25, 4.0);
                }
            }
            Command::SetEq { deck, gains } => {
                let fs = self.out_rate as f32;
                if let Some(d) = self.decks.get_mut(deck) {
                    d.eq_gains = gains;
                    d.eq_l.configure(fs, gains);
                    d.eq_r.configure(fs, gains);
                }
            }
            Command::SetLoop { deck, range_ms } => {
                if let Some(d) = self.decks.get_mut(deck) {
                    d.loop_range =
                        range_ms.map(|(a, b)| (Self::ms_to_frames(d, a), Self::ms_to_frames(d, b)));
                }
            }
            Command::SetCrossfader(x) => {
                self.crossfader = x.clamp(0.0, 1.0);
                self.crossfade = None; // a manual move cancels an automated fade
            }
            Command::Crossfade {
                target,
                duration_ms,
            } => {
                let target = target.clamp(0.0, 1.0);
                let frames = (duration_ms * self.out_rate as u64 / 1000).max(1);
                self.crossfade = Some(CrossfadeRamp {
                    target,
                    per_frame: (target - self.crossfader) / frames as f32,
                    remaining: frames,
                });
            }
            Command::SetMaster(m) => self.master = m.clamp(0.0, 1.5),
        }
    }

    fn set_playing(&mut self, deck: usize, playing: bool) {
        if let Some(d) = self.decks.get_mut(deck) {
            d.playing = playing && d.audio.is_some();
        }
    }

    /// Renders interleaved output into `out` (length must be a multiple of the
    /// output channel count). Allocation-free.
    pub fn render(&mut self, out: &mut [f32]) {
        let ch = self.out_channels as usize;

        for frame in out.chunks_mut(ch) {
            // Advance an automated crossfade one frame.
            if let Some(r) = &mut self.crossfade {
                if r.remaining > 0 {
                    self.crossfader = (self.crossfader + r.per_frame).clamp(0.0, 1.0);
                    r.remaining -= 1;
                } else {
                    self.crossfader = r.target;
                    self.crossfade = None;
                }
            }
            // Equal-power crossfader curve (recomputed per frame while ramping).
            let theta = self.crossfader * std::f32::consts::FRAC_PI_2;
            let (gb, ga) = theta.sin_cos();
            let xfade = [ga, gb];

            let mut mix_l = 0.0f32;
            let mut mix_r = 0.0f32;
            for (idx, d) in self.decks.iter_mut().enumerate() {
                if !d.playing || d.audio.is_none() {
                    continue;
                }
                let [sl, sr] = d.read_stereo();
                let l = d.eq_l.process(sl) * d.gain * xfade[idx];
                let r = d.eq_r.process(sr) * d.gain * xfade[idx];
                mix_l += l;
                mix_r += r;

                // Advance the playhead.
                let step = d.pitch * d.src_rate() / self.out_rate as f64;
                d.pos += step;
                if let Some((ls, le)) = d.loop_range {
                    if le > ls && d.pos >= le {
                        d.pos = ls + (d.pos - le);
                    }
                }
                if d.pos >= d.frames() {
                    d.pos = d.frames();
                    d.playing = false;
                }
            }
            mix_l *= self.master;
            mix_r *= self.master;
            // Write to all output channels (mono-sum for >2 is uncommon here).
            if ch == 1 {
                frame[0] = (mix_l + mix_r) * 0.5;
            } else {
                frame[0] = mix_l;
                frame[1] = mix_r;
                for c in frame.iter_mut().skip(2) {
                    *c = 0.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn ramp(frames: usize, value: f32) -> TrackAudio {
        TrackAudio {
            samples: Arc::from(vec![value; frames].into_boxed_slice()),
            channels: 1,
            sample_rate: 44_100,
        }
    }

    fn render_block(eng: &mut Engine, frames: usize) -> Vec<f32> {
        let mut buf = vec![0.0f32; frames * 2];
        eng.render(&mut buf);
        buf
    }

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0f32, |m, &x| m.max(x.abs()))
    }

    #[test]
    fn silent_when_not_playing() {
        let mut eng = Engine::new(44_100, 2);
        eng.apply(Command::Load {
            deck: 0,
            audio: Some(ramp(44_100, 0.8)),
        });
        let buf = render_block(&mut eng, 256);
        assert_eq!(peak(&buf), 0.0);
    }

    #[test]
    fn plays_after_play_command() {
        let mut eng = Engine::new(44_100, 2);
        eng.apply(Command::Load {
            deck: 0,
            audio: Some(ramp(44_100, 0.8)),
        });
        eng.apply(Command::SetEq {
            deck: 0,
            gains: [0.5, 0.5, 0.5],
        });
        eng.apply(Command::Play(0));
        let buf = render_block(&mut eng, 1024);
        assert!(peak(&buf) > 0.1, "expected audible output");
        assert!(eng.position_ms(0) > 0);
    }

    #[test]
    fn crossfader_selects_deck() {
        let mut eng = Engine::new(44_100, 2);
        eng.apply(Command::Load {
            deck: 0,
            audio: Some(ramp(44_100, 0.8)),
        });
        eng.apply(Command::Load {
            deck: 1,
            audio: Some(ramp(44_100, 0.8)),
        });
        eng.apply(Command::Play(0));
        eng.apply(Command::Play(1));

        eng.apply(Command::SetCrossfader(0.0)); // full A
        let a = peak(&render_block(&mut eng, 512));
        eng.apply(Command::SetCrossfader(1.0)); // full B
        let b = peak(&render_block(&mut eng, 512));
        assert!(a > 0.1 && b > 0.1);

        // At the extremes the *other* deck must be silent: load silence on B,
        // fade fully to B → output near zero.
        eng.apply(Command::Load {
            deck: 1,
            audio: Some(ramp(44_100, 0.0)),
        });
        eng.apply(Command::Play(1));
        eng.apply(Command::SetCrossfader(1.0));
        let only_b_silent = peak(&render_block(&mut eng, 512));
        assert!(
            only_b_silent < 0.05,
            "deck A should be faded out: {only_b_silent}"
        );
    }

    #[test]
    fn gain_zero_is_silent() {
        let mut eng = Engine::new(44_100, 2);
        eng.apply(Command::Load {
            deck: 0,
            audio: Some(ramp(44_100, 0.8)),
        });
        eng.apply(Command::SetCrossfader(0.0));
        eng.apply(Command::SetGain { deck: 0, gain: 0.0 });
        eng.apply(Command::Play(0));
        assert!(peak(&render_block(&mut eng, 512)) < 1e-4);
    }

    #[test]
    fn pitch_advances_faster() {
        let mut eng = Engine::new(44_100, 2);
        eng.apply(Command::Load {
            deck: 0,
            audio: Some(ramp(441_000, 0.5)),
        });
        eng.apply(Command::SetPitch {
            deck: 0,
            pitch: 2.0,
        });
        eng.apply(Command::Play(0));
        render_block(&mut eng, 4410); // 0.1 s of output
                                      // At 2× pitch, ~0.2 s of source consumed.
        assert!(
            eng.position_ms(0) >= 180 && eng.position_ms(0) <= 220,
            "pos {}",
            eng.position_ms(0)
        );
    }

    #[test]
    fn loop_wraps_position() {
        let mut eng = Engine::new(44_100, 2);
        eng.apply(Command::Load {
            deck: 0,
            audio: Some(ramp(44_100, 0.5)),
        });
        eng.apply(Command::SetLoop {
            deck: 0,
            range_ms: Some((0, 100)),
        }); // 100 ms loop
        eng.apply(Command::Play(0));
        render_block(&mut eng, 44_100); // render 1 s — far past the loop
        assert!(
            eng.position_ms(0) <= 100,
            "position should stay within the loop: {}",
            eng.position_ms(0)
        );
        assert!(eng.is_playing(0), "loop should keep playing");
    }

    #[test]
    fn stops_at_end() {
        let mut eng = Engine::new(44_100, 2);
        eng.apply(Command::Load {
            deck: 0,
            audio: Some(ramp(1000, 0.5)),
        });
        eng.apply(Command::Play(0));
        render_block(&mut eng, 4000); // more than the track length
        assert!(!eng.is_playing(0), "playback should stop at the end");
    }

    #[test]
    fn automated_crossfade_ramps_a_to_b() {
        let mut eng = Engine::new(44_100, 2);
        eng.apply(Command::Load {
            deck: 0,
            audio: Some(ramp(441_000, 0.8)),
        }); // loud A
        eng.apply(Command::Load {
            deck: 1,
            audio: Some(ramp(441_000, 0.0)),
        }); // silent B
        eng.apply(Command::Play(0));
        eng.apply(Command::Play(1));
        eng.apply(Command::SetCrossfader(0.0)); // full A
        assert!(
            peak(&render_block(&mut eng, 256)) > 0.3,
            "A should be audible"
        );

        eng.apply(Command::Crossfade {
            target: 1.0,
            duration_ms: 1000,
        });
        render_block(&mut eng, 22_050); // ~500 ms → about halfway
        assert!(
            (eng.crossfader() - 0.5).abs() < 0.1,
            "midway ~0.5, got {}",
            eng.crossfader()
        );

        render_block(&mut eng, 30_000); // finish the fade
        assert!(
            (eng.crossfader() - 1.0).abs() < 1e-3,
            "fade should reach 1.0"
        );
        assert!(
            peak(&render_block(&mut eng, 256)) < 0.05,
            "faded fully to silent B"
        );
    }

    #[test]
    fn manual_crossfader_cancels_automation() {
        let mut eng = Engine::new(44_100, 2);
        eng.apply(Command::Crossfade {
            target: 1.0,
            duration_ms: 5000,
        });
        eng.apply(Command::SetCrossfader(0.25));
        render_block(&mut eng, 4410);
        assert!(
            (eng.crossfader() - 0.25).abs() < 1e-6,
            "manual set cancels the fade"
        );
    }

    #[test]
    fn cue_jumps_back() {
        let mut eng = Engine::new(44_100, 2);
        eng.apply(Command::Load {
            deck: 0,
            audio: Some(ramp(44_100, 0.5)),
        });
        eng.apply(Command::Play(0));
        render_block(&mut eng, 4410);
        assert!(eng.position_ms(0) > 50);
        eng.apply(Command::Cue(0));
        assert_eq!(eng.position_ms(0), 0);
    }
}
