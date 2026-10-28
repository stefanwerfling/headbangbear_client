# MixTree (headbangbear_client)

Intelligente, graphbasierte DJ-Mixing-Software — nativ, lokal und offline.
Tracks sind Knoten eines interaktiven Musikbaums; Verbindungen sind musikalisch
bewertete Übergänge. MixTree zeigt einen **roten Empfehlungspfad** durch den
Graphen und schlägt den nächsten passenden Track vor.

**Stack:** Rust-Kern (`mixtree-core`) · **native GUI mit egui/eframe**
(cross-platform: Linux/Windows/macOS, GPU-gerendert) · SQLite · symphonia +
rustfft (Analyse) · (Audio-Phase) CPAL.

> **Hinweis zur Architektur:** Die Oberfläche war anfangs ein Web-Prototyp
> (Tauri 2 + React/TypeScript + Cytoscape) und wurde auf eine **reine Rust-GUI**
> (egui) umgestellt — für mehr Performance (GPU-Rendering des Graphen, keine
> WebView, gemeinsame Rust-Typen ohne IPC) und um die WebKitGTK-Abhängigkeit zu
> entfernen. Der Web-Prototyp wurde entfernt; das Projekt ist **reines Rust**.

## Status

- **Phase 1 (Grundgerüst)** — [`PHASE1_SUMMARY.md`](PHASE1_SUMMARY.md): Import,
  lokale Bibliothek, interaktiver Musikgraph mit rotem Pfad, Transition-Scoring,
  Next-Best-Empfehlungen, Decks/Mixer.
- **Phase 2 (Analyse)** — [`PHASE2_SUMMARY.md`](PHASE2_SUMMARY.md): lokale
  DSP-Pipeline (symphonia-Dekodierung, BPM/Beat, Key→Camelot, Energie) über eine
  persistente Job-Queue, versioniert gespeichert, mit manueller Korrektur.
- **Native Rust-GUI** — [`PHASE_GUI_SUMMARY.md`](PHASE_GUI_SUMMARY.md): komplette
  egui-Portierung mit Hintergrund-Analyse-Thread und nativem Ordner-Import.
- **Audio-Engine** — [`PHASE_AUDIO_SUMMARY.md`](PHASE_AUDIO_SUMMARY.md): zwei Decks,
  Equal-Power-Crossfader, 3-Band-EQ, Resampling/Pitch, Beat-Loops, Cue; lock-freie
  Steuerung; Geräteausgabe über CPAL (`--features audio-cpal`).
- **Mixing-Feinschliff** — [`PHASE_MIXING_SUMMARY.md`](PHASE_MIXING_SUMMARY.md):
  **Beatmatching/Sync** (oktav-bewusst), echte **Wellenform** aus den dekodierten
  Samples, **Master-VU-Meter**, Pitch-Anzeige.
- **Songstruktur & Cues** — [`PHASE_STRUCTURE_SUMMARY.md`](PHASE_STRUCTURE_SUMMARY.md):
  **Drop-/Struktur-Erkennung** (Intro/Build/Drop/Breakdown/Outro mit Confidence),
  **Cue-Vorschläge** (Mix In / Drop / Mix Out), Abschnitts- und Cue-Marker auf der
  Wellenform.

## Projektstruktur

```
crates/mixtree-core/    reine Logik (DB, Graph, Scoring, Pfade, Import, Analyse-DSP, Beatmatching)
crates/mixtree-audio/   Echtzeit-Audio-Engine (Decks, Mixer, EQ, Loops, Sync; CPAL-Backend)
crates/mixtree-gui/     native egui/eframe-App (Binary `mixtree`)
```

## Bauen & Starten (native App)

Benötigt nur die Rust-Toolchain. Linux braucht zur Laufzeit OpenGL + X11/Wayland
(Standard auf Desktops); Windows/macOS laufen ohne Zusatzpakete.

```bash
cargo run -p mixtree-gui                      # native App (stille Audio-Engine)
cargo run -p mixtree-gui --features audio-cpal # mit hörbarer Ausgabe (CPAL)
cargo build -p mixtree-gui --release

# Windows-Build (von Linux aus oder nativ), mit Audio:
cargo build -p mixtree-gui --release --features audio-cpal --target x86_64-pc-windows-msvc
```

> Audio: Ohne `audio-cpal` läuft eine **stille** Engine (Transport/Playhead/Mixer
> funktionieren, aber kein Ton auf die Lautsprecher). `audio-cpal` aktiviert echte
> Ausgabe (Linux benötigt dafür `libasound2-dev`).

## Entwicklung / Prüfungen

```bash
cargo test -p mixtree-core                       # 77 Tests
cargo clippy -p mixtree-core -p mixtree-gui --all-targets
cargo fmt
```

Der (abgelöste) Web-Prototyp lässt sich weiterhin mit `npm run dev` als
Browser-Vorschau starten; der native Tauri-Build braucht `libwebkit2gtk-4.1-dev`
(siehe [`ARCHITECTURE.md`](ARCHITECTURE.md) §2).

## Lizenz

Siehe [`LICENSE`](LICENSE).
