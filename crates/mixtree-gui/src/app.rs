//! Application state and the top-level eframe layout.
//!
//! The GUI holds the library database directly (no IPC) and recomputes the
//! graph / red path / recommendations from `mixtree-core` whenever the filters,
//! goal or current track change.

use mixtree_audio::{AudioHandle, Command as ACmd, TrackAudio};
use mixtree_core::analysis::decode;
use mixtree_core::analysis::job_queue::JobStatus;
use mixtree_core::analysis::worker::{run_pending, AnalysisEngine};
use mixtree_core::configuration::{Configuration, MixGoal};
use mixtree_core::database::{MixtreeDb, TrackFilter};
use mixtree_core::models::{Genre, Track};
use mixtree_core::music_graph::{self, MusicGraph};
use mixtree_core::pathfinding::{self, RecommendedPath};
use mixtree_core::recommendation::{self, Recommendation};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use crate::{theme, views};

/// Audio output sample rate / channels used by the engine.
pub const AUDIO_RATE: u32 = 44_100;
pub const AUDIO_CHANNELS: u16 = 2;

/// Messages from the background analysis thread to the UI.
pub enum AnalysisMsg {
    /// A batch finished; `remaining` jobs are still queued.
    Progress { remaining: i64 },
    /// The queue has drained.
    Done,
}

/// A deck's audio, decoded on a background thread so the window never blocks on
/// a (potentially slow) file decode. `gen` lets a stale decode be dropped when a
/// newer track was loaded onto the same deck in the meantime.
pub struct DeckAudioLoaded {
    deck: usize,
    generation: u64,
    audio: Option<TrackAudio>,
    waveform: Vec<f32>,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Which main screen is shown in the center.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Tree,
    Playlist,
    Library,
    Settings,
}

/// The loaded state of one deck. Playback position/playing come from the audio
/// engine; this tracks what is loaded, the loop state and the pitch (tempo).
#[derive(Clone)]
pub struct Deck {
    pub track: Option<Track>,
    pub loop_active: bool,
    /// Playback-rate multiplier (`1.0` = original tempo).
    pub pitch: f64,
    /// Normalized waveform envelope of the loaded audio.
    pub waveform: Vec<f32>,
    /// Cue points (manual + suggested) for the loaded track.
    pub cues: Vec<mixtree_core::models::CuePoint>,
    /// Detected structure sections for the loaded track.
    pub sections: Vec<mixtree_core::models::Section>,
}

impl Default for Deck {
    fn default() -> Self {
        Deck {
            track: None,
            loop_active: false,
            pitch: 1.0,
            waveform: Vec::new(),
            cues: Vec::new(),
            sections: Vec::new(),
        }
    }
}

impl Deck {
    /// Effective tempo after pitch, or `None` if the track BPM is unknown.
    pub fn effective_bpm(&self) -> Option<f64> {
        self.track
            .as_ref()
            .and_then(|t| t.bpm)
            .map(|b| b * self.pitch)
    }
}

/// The whole application.
pub struct MixtreeApp {
    pub db: MixtreeDb,
    pub live: bool,

    // Cached reference data.
    pub genres: Vec<Genre>,
    pub config: Configuration,

    // Derived, recomputed on `dirty`.
    pub tracks: Vec<Track>,
    pub graph: MusicGraph,
    pub path: Option<RecommendedPath>,
    pub recs: Vec<Recommendation>,

    // UI state.
    pub view: View,
    pub selected_genres: Vec<String>,
    pub search: String,
    pub bpm_range: (f64, f64),
    pub goal: MixGoal,
    pub current: Option<i64>,
    pub deck_a: Deck,
    pub deck_b: Deck,
    pub played: Vec<i64>,
    pub collapsed_genres: HashSet<String>,

    // Mix-tree view transform.
    pub graph_pan: egui::Vec2,
    pub graph_zoom: f32,

    // Mixer controls (UI state; wired to audio in the audio phase).
    pub eq_a: [f32; 3],
    pub eq_b: [f32; 3],
    pub crossfader: f32,

    // Library inline edit.
    pub editing: Option<i64>,
    pub edit_bpm: String,
    pub edit_key: String,

    // Status line.
    pub status: String,

    // Persistence + background analysis.
    pub db_path: Option<PathBuf>,
    pub analyzing: bool,
    analysis_rx: Option<Receiver<AnalysisMsg>>,

    // Audio engine.
    pub audio: AudioHandle,

    /// When true, disables continuous tree animations (accessibility).
    pub reduced_motion: bool,

    /// Cached cover-art textures per track id (for the tree node discs).
    pub node_textures: std::collections::HashMap<i64, egui::TextureHandle>,
    /// Track ids whose cover decode has already been requested (so each file is
    /// read at most once); the decoded image arrives via `cover_rx`.
    pub cover_requested: HashSet<i64>,
    /// Finished cover decodes from background threads: `(track id, image)`.
    pub cover_rx: Receiver<(i64, egui::ColorImage)>,
    pub cover_tx: Sender<(i64, egui::ColorImage)>,

    // Background deck-audio decoding.
    deck_audio_rx: Receiver<DeckAudioLoaded>,
    deck_audio_tx: Sender<DeckAudioLoaded>,
    /// Per-deck load generation; a decode result is applied only if still current.
    deck_gen: [u64; 2],
    /// Whether a deck's audio is still being decoded (drives repaint + spinner).
    pub deck_loading: [bool; 2],

    /// Dev/test hook: when set (via `MIXTREE_SCREENSHOT=<path>`), render a few
    /// frames then save one PNG of the whole window and exit. Lets a headless
    /// run capture the UI for visual review.
    screenshot_path: Option<PathBuf>,
    screenshot_tick: u32,

    dirty: bool,
}

impl MixtreeApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);

        let (db, live, db_path) = open_database();
        let genres = db.list_genres().unwrap_or_default();
        let config = db.get_config().unwrap_or_default();

        let (cover_tx, cover_rx) = mpsc::channel();
        let (deck_audio_tx, deck_audio_rx) = mpsc::channel();

        let mut app = MixtreeApp {
            db,
            live,
            genres,
            config,
            tracks: Vec::new(),
            graph: MusicGraph::default(),
            path: None,
            recs: Vec::new(),
            view: View::Tree,
            selected_genres: Vec::new(),
            search: String::new(),
            bpm_range: (60.0, 220.0),
            goal: MixGoal::Neutral,
            current: None,
            deck_a: Deck::default(),
            deck_b: Deck::default(),
            played: Vec::new(),
            collapsed_genres: HashSet::new(),
            graph_pan: egui::Vec2::ZERO,
            graph_zoom: 1.0,
            eq_a: [0.5, 0.5, 0.5],
            eq_b: [0.5, 0.5, 0.5],
            crossfader: 0.5,
            editing: None,
            edit_bpm: String::new(),
            edit_key: String::new(),
            status: String::new(),
            db_path,
            analyzing: false,
            analysis_rx: None,
            audio: mixtree_audio::start_default(AUDIO_RATE, AUDIO_CHANNELS),
            reduced_motion: false,
            node_textures: std::collections::HashMap::new(),
            cover_requested: HashSet::new(),
            cover_rx,
            cover_tx,
            deck_audio_rx,
            deck_audio_tx,
            deck_gen: [0, 0],
            deck_loading: [false, false],
            screenshot_path: std::env::var_os("MIXTREE_SCREENSHOT").map(PathBuf::from),
            screenshot_tick: 0,
            dirty: true,
        };
        app.reload();
        // Dev/test aid: auto-play deck A so headless screenshots can show the
        // beat ring. Off by default; only when MIXTREE_AUTOPLAY is set.
        if std::env::var_os("MIXTREE_AUTOPLAY").is_some() {
            app.audio.send(ACmd::Play(0));
        }
        // When importing a folder (dev hook), kick off background analysis.
        if std::env::var_os("MIXTREE_IMPORT").is_some()
            || std::env::var_os("MIXTREE_IMPORT_PERSIST").is_some()
        {
            app.start_analysis();
        }
        app
    }

    /// Loads a track's audio onto an engine deck (0 = A, 1 = B), resetting pitch.
    ///
    /// Cues/sections (cheap DB reads) are applied immediately so the deck shows
    /// the track right away. Real-file decoding — potentially slow — runs on a
    /// background thread; the samples and waveform are applied once the decode
    /// finishes (see `poll_deck_audio`), so the UI never blocks. The synthetic
    /// `sample://` tones are cheap and stay inline.
    fn load_deck_audio(&mut self, deck: usize, track: &Track) {
        let cues = self.db.list_cues(track.id).unwrap_or_default();
        let sections = self.db.list_sections(track.id).unwrap_or_default();
        self.deck_gen[deck] = self.deck_gen[deck].wrapping_add(1);
        let generation = self.deck_gen[deck];
        {
            let d = if deck == 0 {
                &mut self.deck_a
            } else {
                &mut self.deck_b
            };
            d.cues = cues;
            d.sections = sections;
            d.waveform = Vec::new();
        }
        self.set_pitch(deck, 1.0);

        if track
            .file_path
            .starts_with(mixtree_core::sample_data::SAMPLE_PATH_PREFIX)
        {
            let audio = synth_tone(track.bpm.unwrap_or(120.0), 30.0);
            let waveform = audio.peaks(120);
            let d = if deck == 0 {
                &mut self.deck_a
            } else {
                &mut self.deck_b
            };
            d.waveform = waveform;
            self.audio.send(ACmd::Load {
                deck,
                audio: Some(audio),
            });
            self.deck_loading[deck] = false;
            return;
        }

        // Real file: decode off-thread so the window stays responsive.
        self.deck_loading[deck] = true;
        let path = track.file_path.clone();
        let tx = self.deck_audio_tx.clone();
        std::thread::spawn(move || {
            let audio = decode::decode_interleaved(Path::new(&path))
                .ok()
                .map(|a| TrackAudio {
                    samples: Arc::from(a.samples.into_boxed_slice()),
                    channels: a.channels,
                    sample_rate: a.sample_rate,
                });
            let waveform = audio.as_ref().map(|a| a.peaks(120)).unwrap_or_default();
            let _ = tx.send(DeckAudioLoaded {
                deck,
                generation,
                audio,
                waveform,
            });
        });
    }

    /// Applies finished background deck decodes; call once per frame. A result is
    /// dropped if a newer track was loaded onto that deck meanwhile.
    fn poll_deck_audio(&mut self) {
        while let Ok(msg) = self.deck_audio_rx.try_recv() {
            if self.deck_gen[msg.deck] != msg.generation {
                continue; // stale: a newer load superseded this one.
            }
            {
                let d = if msg.deck == 0 {
                    &mut self.deck_a
                } else {
                    &mut self.deck_b
                };
                d.waveform = msg.waveform;
            }
            self.audio.send(ACmd::Load {
                deck: msg.deck,
                audio: msg.audio,
            });
            self.deck_loading[msg.deck] = false;
        }
    }

    /// Sets a deck's pitch (tempo) and informs the engine.
    pub fn set_pitch(&mut self, deck: usize, pitch: f64) {
        if deck == 0 {
            self.deck_a.pitch = pitch;
        } else {
            self.deck_b.pitch = pitch;
        }
        self.audio.send(ACmd::SetPitch { deck, pitch });
    }

    /// Starts an automated transition from deck A to deck B (spec §6): seeks B to
    /// its suggested "Mix In" cue, plays both decks and crossfades over 16 beats.
    pub fn auto_mix(&mut self) {
        if self.deck_b.track.is_none() {
            self.status = "Auto-Mix: zuerst einen Track in Deck B laden.".into();
            return;
        }
        let mix_in = self
            .deck_b
            .cues
            .iter()
            .find(|c| c.label.as_deref() == Some("Mix In"))
            .map(|c| c.position_ms)
            .unwrap_or(0);
        let bpm = self
            .deck_a
            .effective_bpm()
            .or_else(|| self.deck_b.effective_bpm())
            .unwrap_or(128.0);
        let duration_ms = (16.0 * 60_000.0 / bpm) as u64;
        self.audio.send(ACmd::Seek {
            deck: 1,
            ms: mix_in,
        });
        self.audio.send(ACmd::Play(0));
        self.audio.send(ACmd::Play(1));
        self.audio.send(ACmd::Crossfade {
            target: 1.0,
            duration_ms,
        });
        self.status = format!("Auto-Mix A ▶ B über 16 Takte (~{} s)", duration_ms / 1000);
    }

    /// Beatmatches a deck to the other deck's effective tempo (spec §5).
    pub fn sync_deck(&mut self, deck: usize) {
        let other = 1 - deck;
        let master = self.deck(other).effective_bpm();
        let deck_bpm = self.deck(deck).track.as_ref().and_then(|t| t.bpm);
        if let (Some(m), Some(d)) = (master, deck_bpm) {
            let p = mixtree_core::beatmatch::sync_pitch(m, d);
            self.set_pitch(deck, p);
            self.status = format!(
                "Deck {} auf {:.1} BPM synchronisiert",
                if deck == 0 { "A" } else { "B" },
                m
            );
        } else {
            self.status = "Sync benötigt BPM auf beiden Decks".into();
        }
    }

    /// Deck index → `Track` currently loaded there, for UI read-back.
    pub fn deck(&self, deck: usize) -> &Deck {
        if deck == 0 {
            &self.deck_a
        } else {
            &self.deck_b
        }
    }

    /// Sends a command to the audio engine.
    pub fn audio_send(&mut self, cmd: ACmd) {
        self.audio.send(cmd);
    }

    /// The current filter from the sidebar controls.
    pub fn filter(&self) -> TrackFilter {
        TrackFilter {
            search: if self.search.trim().is_empty() {
                None
            } else {
                Some(self.search.clone())
            },
            genres: self.selected_genres.clone(),
            bpm_min: Some(self.bpm_range.0),
            bpm_max: Some(self.bpm_range.1),
            energy_min: None,
            energy_max: None,
            exclude_missing: false,
        }
    }

    /// Marks derived data stale so it is recomputed next frame.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Recomputes tracks, graph, red path and recommendations from the core.
    pub fn reload(&mut self) {
        let filter = self.filter();
        self.tracks = self.db.list_tracks(&filter).unwrap_or_default();

        let start = self
            .current
            .filter(|id| self.tracks.iter().any(|t| t.id == *id))
            .or_else(|| self.tracks.first().map(|t| t.id));
        if self.current.is_none() {
            self.current = start;
            if let Some(t) = start.and_then(|id| self.track(id)) {
                self.deck_a.track = Some(t.clone());
                self.load_deck_audio(0, &t);
            }
        }

        match start {
            Some(start) => {
                let (graph, path) = music_graph::build_graph_with_path(
                    &self.tracks,
                    &self.genres,
                    &self.config,
                    start,
                    &self.goal,
                );
                self.graph = graph;
                self.path = pathfinding::recommend_path(
                    &self.tracks,
                    &self.genres,
                    start,
                    &self.goal,
                    &self.config.weights,
                    &HashSet::new(),
                    5,
                );
                let _ = path;
                if let Some(cur) = self.track(start) {
                    self.recs = recommendation::next_best_tracks(
                        &cur,
                        &self.tracks,
                        &self.genres,
                        &self.goal,
                        &self.config.weights,
                        &self.played.iter().copied().collect(),
                        4,
                    );
                }
            }
            None => {
                self.graph = music_graph::build_graph(&self.tracks, &self.genres, &self.config);
                self.path = None;
                self.recs.clear();
            }
        }
        self.dirty = false;
    }

    /// Looks up a track by id in the current filtered set.
    pub fn track(&self, id: i64) -> Option<Track> {
        self.tracks.iter().find(|t| t.id == id).cloned()
    }

    /// Selects a track as the current track and loads it into deck A.
    pub fn set_current(&mut self, id: i64) {
        self.current = Some(id);
        if let Some(t) = self.track(id) {
            self.deck_a.track = Some(t.clone());
            self.load_deck_audio(0, &t);
        }
        self.mark_dirty();
    }

    /// Loads a track into deck B as the planned next track.
    pub fn load_next(&mut self, id: i64) {
        if let Some(t) = self.track(id) {
            self.deck_b.track = Some(t.clone());
            self.load_deck_audio(1, &t);
            if !self.played.contains(&id) {
                self.played.push(id);
            }
            self.mark_dirty();
        }
    }

    /// Number of queued analysis jobs (for the header indicator).
    pub fn queued_jobs(&self) -> i64 {
        self.db.count_jobs(JobStatus::Queued).unwrap_or(0)
    }

    /// Opens a native folder picker and imports it (metadata only; BPM/key/energy
    /// are enqueued for the background analysis thread). Import needs the
    /// persistent (file-backed) database.
    pub fn import_folder_dialog(&mut self) {
        let Some(folder) = rfd::FileDialog::new()
            .set_title("Musikordner importieren")
            .pick_folder()
        else {
            return;
        };
        let now = now_secs();
        match mixtree_core::music_library::import_folder(&self.db, &folder, now, |_p| {}) {
            Ok(report) => {
                self.status = format!(
                    "Import: {} neu, {} aktualisiert, {} Dubletten, {} Fehler",
                    report.imported,
                    report.updated,
                    report.duplicates,
                    report.errors.len()
                );
                self.mark_dirty();
                self.start_analysis();
            }
            Err(e) => self.status = format!("Import-Fehler: {e}"),
        }
    }

    /// Spawns the background analysis thread (if not already running and the
    /// library is file-backed). The thread opens its own connection and drains
    /// the job queue, decoding audio and running the detectors.
    pub fn start_analysis(&mut self) {
        if self.analyzing {
            return;
        }
        let Some(path) = self.db_path.clone() else {
            self.status = "Analyse benötigt die persistente Bibliothek.".into();
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.analysis_rx = Some(rx);
        self.analyzing = true;
        std::thread::spawn(move || run_analysis_thread(&path, tx));
    }

    /// Drains messages from the analysis thread; call once per frame.
    fn poll_analysis(&mut self) {
        let mut done = false;
        if let Some(rx) = &self.analysis_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    AnalysisMsg::Progress { remaining } => {
                        self.status = format!("Analysiere… {remaining} offen");
                        self.dirty = true;
                    }
                    AnalysisMsg::Done => done = true,
                }
            }
        }
        if done {
            self.analyzing = false;
            self.analysis_rx = None;
            self.status = "Analyse abgeschlossen".into();
            self.dirty = true;
        }
    }

    /// Toggles a genre filter selection.
    pub fn toggle_genre(&mut self, id: &str) {
        if let Some(pos) = self.selected_genres.iter().position(|g| g == id) {
            self.selected_genres.remove(pos);
        } else {
            self.selected_genres.push(id.to_string());
        }
        self.mark_dirty();
    }
}

/// Saves an egui framebuffer image as a PNG (dev screenshot hook).
fn save_screenshot(img: &egui::ColorImage, path: &Path) {
    let [w, h] = img.size;
    let mut bytes = Vec::with_capacity(w * h * 4);
    for p in &img.pixels {
        bytes.extend_from_slice(&p.to_array());
    }
    match image::save_buffer(path, &bytes, w as u32, h as u32, image::ExtendedColorType::Rgba8) {
        Ok(()) => eprintln!("screenshot saved: {} ({w}x{h})", path.display()),
        Err(e) => eprintln!("screenshot failed: {e}"),
    }
}

/// Synthesizes a short, clearly-labelled test tone (mono) for the development
/// `sample://` tracks so the transport, playhead and mixer are demonstrable
/// without real audio files. A soft tonic sine plus a beat click at `bpm`.
fn synth_tone(bpm: f64, seconds: f64) -> TrackAudio {
    let rate = AUDIO_RATE;
    let n = (rate as f64 * seconds) as usize;
    let beat_period = (60.0 / bpm.max(1.0) * rate as f64) as usize;
    let mut s = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let tone = (2.0 * std::f32::consts::PI * 110.0 * t).sin() * 0.12;
        let into_beat = if beat_period > 0 { i % beat_period } else { 0 };
        let click = if into_beat < 800 {
            let env = 1.0 - into_beat as f32 / 800.0;
            (2.0 * std::f32::consts::PI * 1500.0 * t).sin() * 0.25 * env
        } else {
            0.0
        };
        s.push(tone + click);
    }
    TrackAudio {
        samples: Arc::from(s.into_boxed_slice()),
        channels: 1,
        sample_rate: rate,
    }
}

/// Background worker: opens its own DB connection and drains the analysis queue.
fn run_analysis_thread(path: &Path, tx: mpsc::Sender<AnalysisMsg>) {
    let Ok(db) = MixtreeDb::open(path) else {
        let _ = tx.send(AnalysisMsg::Done);
        return;
    };
    let engine = AnalysisEngine::default();
    loop {
        let report = run_pending(&db, &engine, 4, now_secs(), decode::decode_for_analysis);
        match report {
            Ok(r) => {
                let remaining = db.count_jobs(JobStatus::Queued).unwrap_or(0);
                let _ = tx.send(AnalysisMsg::Progress { remaining });
                if r.processed == 0 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let _ = tx.send(AnalysisMsg::Done);
}

impl eframe::App for MixtreeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_analysis();
        self.poll_deck_audio();

        // Headless screenshot hook: settle a few frames, then capture + exit.
        if let Some(path) = self.screenshot_path.clone() {
            ctx.request_repaint();
            self.screenshot_tick += 1;
            if self.screenshot_tick == 14 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = shot {
                save_screenshot(&img, &path);
                std::process::exit(0);
            }
        }
        if self.analyzing {
            // Keep polling the worker while analysis runs.
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
        if self.deck_loading[0] || self.deck_loading[1] {
            // A background deck decode is in flight; repaint to pick up the result.
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
        // Animate the playhead while a deck is playing.
        if self.audio.is_playing(0) || self.audio.is_playing(1) {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
        if self.dirty {
            self.reload();
        }

        egui::TopBottomPanel::top("header")
            .exact_height(56.0)
            .frame(egui::Frame::none().fill(theme::BG2))
            .show(ctx, |ui| views::header::show(self, ui));

        // Thin status footer at the very bottom (matches the reference): theme
        // focus on the left, live BPM/Key/Energy of the current track on the right.
        egui::TopBottomPanel::bottom("footer")
            .exact_height(30.0)
            .frame(egui::Frame::none().fill(theme::BG2))
            .show(ctx, |ui| views::footer::show(self, ui));

        egui::TopBottomPanel::bottom("decks")
            .exact_height(236.0)
            .frame(egui::Frame::none().fill(theme::PANEL))
            .show(ctx, |ui| views::decks::show(self, ui));

        egui::SidePanel::left("sidebar")
            .exact_width(240.0)
            .frame(egui::Frame::none().fill(theme::PANEL))
            .show(ctx, |ui| views::sidebar::show(self, ui));

        egui::SidePanel::right("nextbest")
            .exact_width(350.0)
            .frame(egui::Frame::none().fill(theme::PANEL))
            .show(ctx, |ui| views::nextbest::show(self, ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::BG))
            .show(ctx, |ui| match self.view {
                View::Library => views::library::show(self, ui),
                View::Settings => views::settings::show(self, ui),
                View::Playlist => views::playlist::show(self, ui),
                View::Tree => views::graph::show(self, ui),
            });
    }
}

/// Opens the persistent library database in the platform data dir, falling back
/// to an in-memory database if that fails. Seeds sample data into an empty
/// library for first-run (clearly marked `sample://`).
fn open_database() -> (MixtreeDb, bool, Option<PathBuf>) {
    // Dev/test hook: MIXTREE_IMPORT=<folder> opens a fresh in-memory library,
    // imports that folder and runs the full analysis synchronously — so a
    // headless screenshot shows real tracks, covers and analysed BPM/energy.
    // One-off: import a folder into the PERSISTENT library (replacing any sample
    // data), so afterwards a normal launch shows that music permanently.
    if let Some(folder) = std::env::var_os("MIXTREE_IMPORT_PERSIST") {
        if let Some(dirs) = directories::ProjectDirs::from("com", "headbangbear", "MixTree") {
            let dir = dirs.data_dir();
            let _ = std::fs::create_dir_all(dir);
            let db_path = dir.join("mixtree.sqlite");
            for suffix in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{}", db_path.display(), suffix));
            }
            if let Ok(db) = MixtreeDb::open(&db_path) {
                let now = now_secs();
                let _ =
                    mixtree_core::music_library::import_folder(&db, PathBuf::from(folder), now, |_| {});
                return (db, true, Some(db_path));
            }
        }
    }

    if let Some(folder) = std::env::var_os("MIXTREE_IMPORT") {
        // Fresh temp DB (no sample seeding), import the folder (fast, metadata +
        // covers). Analysis runs in the background thread once the window is up.
        let tmp = std::env::temp_dir().join("mixtree-import.sqlite");
        let _ = std::fs::remove_file(&tmp);
        let db = MixtreeDb::open(&tmp).expect("temp db");
        let now = now_secs();
        let _ = mixtree_core::music_library::import_folder(&db, PathBuf::from(folder), now, |_| {});
        return (db, true, Some(tmp));
    }

    let path = directories::ProjectDirs::from("com", "headbangbear", "MixTree").and_then(|dirs| {
        let dir = dirs.data_dir();
        std::fs::create_dir_all(dir).ok()?;
        Some(dir.join("mixtree.sqlite"))
    });
    match path.as_ref().and_then(|p| MixtreeDb::open(p).ok()) {
        Some(db) => {
            let _ = mixtree_core::seed_sample_tracks_if_empty(&db);
            (db, true, path)
        }
        None => {
            let db = MixtreeDb::open_in_memory().expect("in-memory db");
            let _ = mixtree_core::seed_sample_tracks_if_empty(&db);
            (db, false, None)
        }
    }
}
