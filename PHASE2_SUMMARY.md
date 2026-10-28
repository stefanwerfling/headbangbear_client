# MixTree — Zusammenfassung Phase 2 (Analyse)

Stand: 2026-10-28. Format nach Spec §14. Baut auf [`PHASE1_SUMMARY.md`](PHASE1_SUMMARY.md) auf.

## Implementierte Funktionen

- **Audio-Dekodierung** (`analysis/decode`): `symphonia` dekodiert MP3/FLAC/WAV/
  OGG/M4A/AAC zu Mono-`f32`; für die Analyse wird auf 11 025 Hz heruntergetaktet.
  Reine Rust-Lösung, keine System- oder Python-Abhängigkeiten (Spec §2).
- **DSP-Grundbausteine** (`analysis/dsp`): Hann-Fenster, Block-Downsampling, RMS
  und ein Kurzzeit-Magnitudenspektrogramm (FFT via `rustfft`).
- **BPM-/Beat-Erkennung** (`analysis/bpm`): Spektralfluss-Onset-Hüllkurve →
  Autokorrelation über 60–200 BPM → parabolische Peak-Interpolation (Sub-Lag-
  Genauigkeit) → Kammfilter-Phase für das **Beat-Grid**. Liefert BPM,
  Halb-/Doppelzeit-Kandidaten, Beat-Positionen (ms) und eine Confidence.
- **Key-Erkennung** (`analysis/key_detection`): 12-Bin-Chroma + Krumhansl-
  Schmuckler-Profilkorrelation über alle 24 Tonarten → **Camelot** (z. B. `8A`),
  mit Confidence aus dem Abstand zum Zweitplatzierten.
- **Energie-Schätzung** (`analysis/energy`): Kombination aus Lautheit (RMS, dB),
  Helligkeit (spektraler Schwerpunkt) und rhythmischer Aktivität (Spektralfluss)
  → `0–100`. **Immer als Schätzung markiert** (`estimated: true`) — Lautheit ≠
  musikalische Energie (Spec §7).
- **Analyse-Worker** (`analysis/worker`): arbeitet die persistente Job-Queue ab,
  dekodiert jede Datei **einmal pro Durchlauf** (Cache), führt den passenden
  Detektor aus und schreibt **versionierte** Ergebnisse (bpm/key/energy,
  `analysis_version`, `analysis_state`) zurück. Fehler markieren Job und Track
  sauber als `failed`, ohne den Durchlauf abzubrechen.
- **Tauri-Commands**: `run_analysis_batch(limit)` (verarbeitet Jobs, Event
  `analysis://done`), `reanalyze_track(id)` (Jobs neu einreihen),
  `correct_track(id, bpm?, key?)` (manuelle Korrektur, Spec §7).
- **UI**: Header zeigt eine **„Analysieren (N)"**-Pille mit offenen Jobs und
  startet die Verarbeitung batchweise; die Bibliothek erlaubt **manuelle BPM-/
  Key-Korrektur** (Inline-Felder ✎/✓) und **Neu-Analyse** (↻) pro Track.
- **Import-Kopplung**: Bereits in Phase 1 reiht der Import automatisch Jobs für
  fehlende Features ein — Phase 2 verarbeitet sie jetzt wirklich.

## Geänderte / neue Module

- Neu in `crates/mixtree-core/src/analysis/`: `decode`, `dsp`, `bpm`,
  `key_detection`, `energy`, `worker`.
- `crates/mixtree-core/Cargo.toml`: `symphonia`, `rustfft`.
- Neu `src-tauri/src/commands/analysis.rs`; registriert in `commands/mod.rs`
  und `lib.rs`.
- Frontend: `App.tsx` (Job-Status, Analyse-/Korrektur-Handler),
  `components/Header.tsx` (Analyse-Pille), `components/LibraryView.tsx`
  (Inline-Korrektur + Neu-Analyse), `lib/{api,types}.ts`, `styles.css`.
- Zusätzlicher visueller Feinschliff des Musikbaums (runde Genre-farbige
  Knoten, BPM-Pills auf Kanten, kräftigerer roter Pfad) näher am Designbild.

## Ausgeführte Tests & statische Prüfungen

```
cargo test  -p mixtree-core             → 77 passed; 0 failed   (Phase 1: 54, +23 neu)
cargo clippy -p mixtree-core --all-targets → 0 warnings
cargo fmt   -p mixtree-core --check      → sauber
npx tsc --noEmit                         → 0 Fehler
npx eslint . --max-warnings 0            → 0 Fehler/Warnungen
npx vite build                           → erfolgreich
```

Neue DSP-Tests u. a.: FFT-Peak an Eingangsfrequenz, Downsampling, RMS;
WAV-Dekodierung (selbst erzeugte Datei) + beschädigte Datei; BPM-Erkennung auf
synthetischen Click-Trains bei **120/128/140 BPM** inkl. Beat-Grid-Abstand;
Chroma-Peak bei A440 und Key-Erkennung eines C-Dur-Dreiklangs; Energie-Ordnung
(laut > leise); Worker-End-to-End (einmal dekodieren, bpm/key/energy füllen,
Queue leeren, Fehlerpfad markiert Job+Track `failed`).

## Testergebnisse

Alle Prüfungen grün. Die Detektoren sind mit synthetischen Signalen mit bekanntem
Grundwert verifiziert; der Worker ist end-to-end gegen eine echte WAV-Dekodierung
und über eine injizierte Decode-Closure getestet.

## Bekannte Einschränkungen

- **Desktop-Build** weiterhin nur mit `libwebkit2gtk-4.1-dev` (siehe
  `ARCHITECTURE.md` §2); `src-tauri` inkl. der neuen Analyse-Commands wurde hier
  nicht kompiliert. Die Analyse-Logik ist jedoch vollständig im Core getestet.
- **Browser-Demo** kann nicht analysieren (keine echten Dateien) — die
  Analyse-Pille/Buttons sind dort entsprechend inaktiv; `correct_track` wirkt im
  Browser auf die In-Memory-Beispieldaten.
- **Genauigkeit**: Autokorrelation kann bei einigen Tracks Halb-/Doppelzeit
  wählen (Kandidaten + manuelle Korrektur vorhanden); Key-Erkennung ist bei
  tonal mehrdeutigem Material unsicher (Confidence wird mitgeliefert).
- **Songstruktur/Drops** (Intro/Build/Drop/Breakdown/Outro) ist noch nicht
  implementiert — `AnalysisKind::Structure` ist als No-Op-Erfolg vorgesehen und
  folgt in Phase 5.
- Keine Hintergrund-**Thread**-Automatik: die Verarbeitung wird aktuell per
  Command batchweise angestoßen (UI-gesteuert); ein dedizierter Worker-Thread mit
  Live-Fortschritt ist ein kleiner nächster Schritt.

## Nächste konkrete Arbeitsschritte

- **Phase 3 (Transition Engine)** ist im Kern bereits vorhanden (Scoring, Graph,
  roter Pfad, Next-Best) — als Nächstes: erklärbare Pfad-Kompromisse in der UI
  vertiefen und gelernte Gewichte (Spec §9) einbeziehen.
- **Phase 4 (Audio-Engine)**: CPAL-Ausgabe, zwei Decks, Wiedergabe/Cue/Loop,
  echte Wellenformen, Crossfader/EQ an den Audiostrom koppeln (benötigt
  `libasound2-dev`).
- Analyse-Worker-Thread mit `analysis://progress`-Events und automatischem Start
  nach dem Import.
