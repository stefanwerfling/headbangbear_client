//! # MixTree Audio
//!
//! The real-time audio engine: two decks, an equal-power crossfader, per-deck
//! 3-band EQ, resampling (pitch/tempo) and beat loops. Control flows from the UI
//! to the audio thread over a lock-free SPSC queue; the audio thread owns the
//! [`engine::Engine`] and renders blocks to a pluggable [`backend::Backend`].
//!
//! This crate is device-independent and fully unit-tested. Real device output
//! is a backend choice: [`start_null`] (silent, paced), [`start_pipe`]
//! (`pacat`), or [`start_cpal`] (feature `cpal`, cross-platform incl. Windows).

#![forbid(unsafe_code)]

pub mod backend;
pub mod command;
pub mod engine;
pub mod eq;

pub use command::{Command, TrackAudio};

use backend::{Backend, NullBackend, PipeBackend};
use engine::Engine;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// How many frames the audio thread renders per block.
const BLOCK_FRAMES: usize = 512;
/// Capacity of the command queue.
const CMD_CAPACITY: usize = 256;

/// Shared, lock-free snapshot the audio thread publishes for the UI.
struct Shared {
    positions_ms: [AtomicU32; 2],
    playing: [AtomicBool; 2],
    /// Master output peak of the last block, as `f32` bits (for a VU meter).
    master_peak: AtomicU32,
    stop: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Shared {
            positions_ms: [AtomicU32::new(0), AtomicU32::new(0)],
            playing: [AtomicBool::new(false), AtomicBool::new(false)],
            master_peak: AtomicU32::new(0),
            stop: AtomicBool::new(false),
        }
    }
}

/// Peak absolute amplitude of a block.
fn block_peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, &x| m.max(x.abs()))
}

/// A handle for controlling the audio thread and reading playback state.
pub struct AudioHandle {
    tx: rtrb::Producer<Command>,
    shared: Arc<Shared>,
    out_rate: u32,
    /// Human-readable name of the active backend.
    pub backend_name: &'static str,
}

impl AudioHandle {
    /// Sends a command to the audio thread (dropped silently if the queue is full).
    pub fn send(&mut self, cmd: Command) {
        let _ = self.tx.push(cmd);
    }

    /// Current playhead of a deck in milliseconds.
    pub fn position_ms(&self, deck: usize) -> u64 {
        self.shared
            .positions_ms
            .get(deck)
            .map(|p| p.load(Ordering::Relaxed) as u64)
            .unwrap_or(0)
    }

    /// Whether a deck is currently playing.
    pub fn is_playing(&self, deck: usize) -> bool {
        self.shared
            .playing
            .get(deck)
            .map(|p| p.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// Output sample rate.
    pub fn out_rate(&self) -> u32 {
        self.out_rate
    }

    /// Master output peak (`0.0..`) of the most recent block, for a VU meter.
    pub fn master_peak(&self) -> f32 {
        f32::from_bits(self.shared.master_peak.load(Ordering::Relaxed))
    }
}

impl Drop for AudioHandle {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

/// Spawns the audio thread driving `backend`, returning a control handle.
fn start_with_backend(
    mut backend: Box<dyn Backend>,
    out_rate: u32,
    out_channels: u16,
    backend_name: &'static str,
) -> AudioHandle {
    let (tx, mut rx) = rtrb::RingBuffer::<Command>::new(CMD_CAPACITY);
    let shared = Arc::new(Shared::new());
    let thread_shared = shared.clone();

    std::thread::Builder::new()
        .name("mixtree-audio".into())
        .spawn(move || {
            let mut engine = Engine::new(out_rate, out_channels);
            let mut buf = vec![0.0f32; BLOCK_FRAMES * out_channels.max(1) as usize];
            while !thread_shared.stop.load(Ordering::Relaxed) {
                while let Ok(cmd) = rx.pop() {
                    engine.apply(cmd);
                }
                engine.render(&mut buf);
                backend.submit(&buf);
                thread_shared
                    .master_peak
                    .store(block_peak(&buf).to_bits(), Ordering::Relaxed);
                for deck in 0..2 {
                    thread_shared.positions_ms[deck]
                        .store(engine.position_ms(deck) as u32, Ordering::Relaxed);
                    thread_shared.playing[deck].store(engine.is_playing(deck), Ordering::Relaxed);
                }
            }
        })
        .expect("spawn audio thread");

    AudioHandle {
        tx,
        shared,
        out_rate,
        backend_name,
    }
}

/// Starts a silent, real-time-paced engine (never emits sound). Always succeeds.
pub fn start_null(out_rate: u32, out_channels: u16) -> AudioHandle {
    start_with_backend(
        Box::new(NullBackend::new(out_rate, out_channels)),
        out_rate,
        out_channels,
        "null",
    )
}

/// Starts output via `pacat` (Linux). Returns `None` if `pacat` is unavailable.
pub fn start_pipe(out_rate: u32, out_channels: u16) -> Option<AudioHandle> {
    let backend = PipeBackend::new(out_rate, out_channels)?;
    Some(start_with_backend(
        Box::new(backend),
        out_rate,
        out_channels,
        "pacat",
    ))
}

/// Starts cross-platform output via CPAL (feature `cpal`).
#[cfg(feature = "cpal")]
pub fn start_cpal() -> Option<AudioHandle> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = host.default_output_device()?;
    let config = device.default_output_config().ok()?;
    let out_rate = config.sample_rate().0;
    let out_channels = config.channels();

    let (tx, mut rx) = rtrb::RingBuffer::<Command>::new(CMD_CAPACITY);
    let shared = Arc::new(Shared::new());
    let cb_shared = shared.clone();

    let mut engine = Engine::new(out_rate, out_channels);
    let err_fn = |e| eprintln!("cpal stream error: {e}");
    let stream = device
        .build_output_stream(
            &config.config(),
            move |data: &mut [f32], _| {
                while let Ok(cmd) = rx.pop() {
                    engine.apply(cmd);
                }
                engine.render(data);
                cb_shared
                    .master_peak
                    .store(block_peak(data).to_bits(), Ordering::Relaxed);
                for deck in 0..2 {
                    cb_shared.positions_ms[deck]
                        .store(engine.position_ms(deck) as u32, Ordering::Relaxed);
                    cb_shared.playing[deck].store(engine.is_playing(deck), Ordering::Relaxed);
                }
            },
            err_fn,
            None,
        )
        .ok()?;
    stream.play().ok()?;
    // Keep the stream alive on a detached thread until stop is requested.
    let keep = shared.clone();
    std::thread::spawn(move || {
        let _stream = stream;
        while !keep.stop.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });

    Some(AudioHandle {
        tx,
        shared,
        out_rate,
        backend_name: "cpal",
    })
}

/// Picks the best available backend: CPAL (if compiled) → `pacat`
/// (PulseAudio/PipeWire, Linux) → silent Null fallback.
///
/// The Null fallback keeps the engine/clock running without routing audio to the
/// speakers, which is the right behaviour for headless/CI environments (where no
/// sound server is reachable, so `pacat` fails and we fall through to it anyway).
/// Set `MIXTREE_AUDIO=null` to force the silent backend explicitly.
pub fn start_default(out_rate: u32, out_channels: u16) -> AudioHandle {
    if std::env::var("MIXTREE_AUDIO").as_deref() == Ok("null") {
        return start_null(out_rate, out_channels);
    }
    #[cfg(feature = "cpal")]
    {
        if let Some(h) = start_cpal() {
            return h;
        }
    }
    // Linux desktops: stream to PulseAudio/PipeWire via `pacat` (no build-time
    // audio dev headers needed). Returns None if `pacat`/the sound server is
    // unavailable, in which case we fall through to the silent engine.
    if let Some(h) = start_pipe(out_rate, out_channels) {
        return h;
    }
    start_null(out_rate, out_channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_handle_starts_and_reports_idle() {
        let h = start_null(44_100, 2);
        assert_eq!(h.out_rate(), 44_100);
        assert!(!h.is_playing(0));
        assert_eq!(h.position_ms(0), 0);
        assert_eq!(h.backend_name, "null");
    }

    #[test]
    fn commands_reach_the_engine() {
        let mut h = start_null(44_100, 2);
        let audio = TrackAudio {
            samples: Arc::from(vec![0.5f32; 44_100].into_boxed_slice()),
            channels: 1,
            sample_rate: 44_100,
        };
        h.send(Command::Load {
            deck: 0,
            audio: Some(audio),
        });
        h.send(Command::Play(0));
        // Give the audio thread a moment to process and advance.
        std::thread::sleep(std::time::Duration::from_millis(120));
        assert!(h.is_playing(0), "deck should be playing");
        assert!(h.position_ms(0) > 0, "playhead should have advanced");
    }
}
