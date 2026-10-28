//! Output backends. The audio thread renders blocks and hands them to a
//! [`Backend::submit`]; each backend decides where the samples go.
//!
//! - [`NullBackend`]: discards output but paces itself to real time, so the
//!   engine clock (and the UI playhead) advance correctly **without making
//!   sound** — the safe default.
//! - [`PipeBackend`]: streams to `pacat` (PulseAudio/PipeWire) on Linux.
//! - CPAL backend: see [`crate::start_cpal`] (feature `cpal`, cross-platform).

use std::io::Write;
use std::time::{Duration, Instant};

/// Consumes rendered interleaved `f32` blocks from the audio thread.
pub trait Backend: Send {
    /// Hands one rendered block to the output.
    fn submit(&mut self, buf: &[f32]);
}

/// Discards audio but paces to real time so the engine advances like playback.
pub struct NullBackend {
    frame_dur: Duration,
    next: Option<Instant>,
}

impl NullBackend {
    /// Creates a backend pacing for the given output format.
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        let _ = channels;
        NullBackend {
            frame_dur: Duration::from_secs_f64(1.0 / sample_rate.max(1) as f64),
            next: None,
        }
    }
}

impl Backend for NullBackend {
    fn submit(&mut self, buf: &[f32]) {
        // buf length is frames * channels; pace by the number of frames.
        let frames = buf.len().max(1);
        let block = self.frame_dur * frames as u32;
        let target = self.next.unwrap_or_else(Instant::now) + block;
        let now = Instant::now();
        if target > now {
            std::thread::sleep(target - now);
        }
        self.next = Some(target.max(now));
    }
}

/// Streams audio to `pacat` (PulseAudio / PipeWire) on Linux.
pub struct PipeBackend {
    child: std::process::Child,
    bytes: Vec<u8>,
}

impl PipeBackend {
    /// Spawns `pacat`. Returns `None` if it cannot be started.
    pub fn new(sample_rate: u32, channels: u16) -> Option<Self> {
        let child = std::process::Command::new("pacat")
            .args([
                "--playback",
                &format!("--rate={sample_rate}"),
                &format!("--channels={channels}"),
                "--format=float32le",
                "--client-name=MixTree",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;
        Some(PipeBackend {
            child,
            bytes: Vec::new(),
        })
    }
}

impl Backend for PipeBackend {
    fn submit(&mut self, buf: &[f32]) {
        self.bytes.clear();
        self.bytes.reserve(buf.len() * 4);
        for &s in buf {
            self.bytes.extend_from_slice(&s.to_le_bytes());
        }
        if let Some(stdin) = self.child.stdin.as_mut() {
            // Backpressure from the pipe paces playback to real time.
            let _ = stdin.write_all(&self.bytes);
        }
    }
}

impl Drop for PipeBackend {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
