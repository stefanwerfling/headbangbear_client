# MixTree — Technischer Architekturplan

MixTree ist eine native, lokal und offline nutzbare DJ-Software mit graphbasierter
Musiknavigation. Dieses Dokument beschreibt die Architektur, die wesentlichen
Designentscheidungen, Risiken und die Datenmodelle. Es ist die Umsetzung von
Spezifikation §10 und §14 (Schritte 2–4).

---

## 1. Überblick & Leitprinzip

Die oberste Architekturregel aus der Spec (§5, §10) lautet: **Audio-Engine,
Analyse, Graphalgorithmen, Datenbank und UI sind so unabhängig wie sinnvoll**.
Genau diese Trennung hat den **GUI-Wechsel** billig gemacht: Die Oberfläche wurde
vom Web-Prototyp (Tauri 2 + React/TypeScript + Cytoscape) auf eine **native
Rust-GUI (egui/eframe)** umgestellt, ohne dass `mixtree-core` sich geändert hat.

```
headbangbear_client/
├── crates/mixtree-core/   # reine Logik, UI- & audiounabhängig, voll getestet
├── crates/mixtree-audio/  # Echtzeit-Audio-Engine (Decks/Mixer/EQ/Loops/Sync)
└── crates/mixtree-gui/    # native egui/eframe-App — nutzt core + audio direkt
```

Der ursprüngliche Web-Prototyp (Tauri 2 + React/TS + Cytoscape) wurde nach dem
Wechsel auf die native GUI vollständig **entfernt**; das Projekt ist reines Rust.

**Warum die native Rust-GUI (egui):** kein WebView → der Graph, die Wellenformen
und Meter werden auf der **GPU** gezeichnet (bei vielen Knoten schneller + geringere
Latenz als Cytoscape im Webview); GUI und Engine teilen **dieselben Rust-Typen**
ohne JSON-über-IPC; cross-platform (Linux/Windows/macOS über winit + glow/OpenGL);
und die WebKitGTK-Build-Abhängigkeit entfällt. egui ist immediate-mode und damit
ideal für custom-gezeichnete Echtzeit-UIs. Der Web-Prototyp bleibt als Referenz.

- **`mixtree-core`** enthält Datenmodelle, SQLite-Bibliothek, Import,
  Transition-Scoring, Musikgraph, Pfadfindung, Empfehlungen, die Analyse-DSP und
  Beatmatching. Es hat **keine** Abhängigkeit zu GUI oder Audio-Gerät und ist auf
  jeder Maschine mit `cargo test` prüfbar.
- **`mixtree-audio`** ist die geräteunabhängige Echtzeit-Audio-Engine (Decks,
  Mixer, EQ, Loops, Sync) mit austauschbaren Ausgabe-Backends.
- **`mixtree-gui`** (Binary `mixtree`) ist die native egui/eframe-Oberfläche. Sie
  nutzt `mixtree-core` und `mixtree-audio` **direkt** (gemeinsame Rust-Typen, kein
  IPC) und hält die Datenbank selbst.

### Warum SQLite und nicht z. B. ScyllaDB

MixTree ist eine **lokale Single-User-Desktop-App** mit Offline-Pflicht (§1).
SQLite (eingebettet, als `bundled` mitkompiliert, kein System-libsqlite nötig)
passt dazu exakt: eine Datei, kein Server, volles SQL für die flexible
BPM-/Energie-/Key-Filterung der Sidebar, µs-Latenz ohne Netzwerk-Hop (wichtig
für §5: keine blockierenden DB-Zugriffe nahe dem Audiothread). Verteilte
Wide-Column-Stores wie ScyllaDB adressieren das Gegenteil (Cluster,
Milliarden-Rows, schreibintensive Server-Workloads) und würden einen
mitzuliefernden Daemon erzwingen. Die DB liegt hinter einem Repository-Layer
(`database/`), ein späterer Austausch bliebe also gekapselt.

---

## 2. Build-Voraussetzungen

| Komponente | Baubar? | Benötigt |
|---|---|---|
| `mixtree-core` | ✅ ja (getestet) | nur Rust-Toolchain (SQLite via `bundled`) |
| **`mixtree-gui`** (native App) | ✅ **ja (läuft + Screenshot)** | Rust-Toolchain + OpenGL/X11-Laufzeit (Standard auf Desktops); Windows/macOS ohne Zusatz |
| **`mixtree-audio`** (Engine) | ✅ **ja (16 Tests)** | nur Rust-Toolchain (Engine); hörbare Ausgabe via Feature `cpal` → Linux: `libasound2-dev` |

Die native GUI ist der Standard. Starten/Bauen:

```bash
cargo run   -p mixtree-gui            # native App
cargo build -p mixtree-gui --release
cargo build -p mixtree-gui --release --target x86_64-pc-windows-msvc  # Windows
```

Hörbare Audioausgabe (Feature `cpal`) braucht auf Linux `libasound2-dev`:

```bash
sudo apt install libasound2-dev          # nur für hörbare Ausgabe (CPAL)
cargo run -p mixtree-gui --features audio-cpal
```

Alle Prüfungen (ohne System-Abhängigkeiten):

```bash
cargo test                       # core + audio (100 Tests)
cargo clippy --all-targets
cargo fmt
```

---

## 3. Modulkarte (`mixtree-core`, spiegelt Spec §10)

| Modul | Aufgabe |
|---|---|
| `models` | `Track`, `Genre`, `Playlist`, `MixHistoryEntry`, Enums |
| `camelot` | Camelot-Wheel-Keys + harmonische Kompatibilität |
| `configuration` | Scoring-Gewichte, `MixGoal`, Default-Genre-Taxonomie |
| `database` | SQLite-Persistenz (`schema` = versionierte Migrationen, Repository) |
| `metadata` | Tag-Extraktion (lofty) + Content-Hash |
| `music_library` | rekursiver Import, Dubletten-/Moved-File-Erkennung |
| `transition_engine` | mehrdimensionales, normalisiertes Übergangs-Scoring |
| `music_graph` | spärlicher Graph aus Tracks + Genre-Gruppen, roter Pfad |
| `pathfinding` | Dijkstra (Zieltrack) + zielgerichtete Beam-Suche (roter Pfad) |
| `recommendation` | Next-Best-Track inkl. Alternativen & Begründung |
| `analysis` | Traits + **Phase-2-DSP**: `decode` (symphonia), `dsp` (FFT/RMS), `bpm`, `key_detection`, `energy`, `worker` + persistente Job-Queue |
| `sample_data` | Entwicklungs-Beispieldaten (klar markiert, `sample://`) |

Die Analyse-Pipeline (Phase 2) ist implementiert: Dekodierung → BPM/Beat-Grid
(Autokorrelation), Key→Camelot (Chroma + Krumhansl-Schmuckler), Energie
(RMS/Centroid/Flux, als Schätzung markiert). Der `worker` arbeitet die Job-Queue
ab und speichert versioniert. Die noch offenen Module `audio_engine`,
`audio_output`, `deck`, `beat_tracking` (Echtzeit), `structure_detection`,
`cue_loop` folgen in den Phasen 4/5; ihre Schnittstellen (Cue-/Loop-Tabellen,
Mix-History-Events, Analyse-Traits) sind bereits angelegt.

---

## 4. Datenmodelle (Kern)

- **Track**: `id, file_path, file_hash, title, artist, album, genre, subgenre,
  bpm?, key?(Camelot), energy?(0–100, geschätzt), duration_ms?, year?, rating?,
  cover_art_path?, date_added, analysis_version, analysis_state, missing`.
  BPM/Key/Energy sind `Option`, weil sie erst durch Tags oder die Phase-2-Analyse
  entstehen. `analysis_version` erlaubt gezielte Neuberechnung bei geänderten
  Algorithmen (§8).
- **CamelotKey**: `{ number: 1..12, is_major }`, Anzeige `8A`/`8B`,
  Kompatibilität normalisiert auf `0..1`.
- **TransitionScore**: `total` plus Teilbewertungen `bpm, key, energy, genre,
  mixability, goal, repetition_penalty`, dazu `bpm_diff` und `notes`
  (Erklärbarkeit).
- **MixGoal**: die sieben Ziele aus §4 (Energie ↑/↓, Genre halten/wechseln,
  harmonisch, überraschend, Zieltrack/-genre) + Neutral.
- **MusicGraph**: `nodes` (Gruppen + Tracks), `edges` (bewertete Übergänge,
  `on_red_path`), `group_links`.

### Scoring-Formel (Spec §4)

```
TransitionScore = ( w_bpm·BPM + w_key·Harmonic + w_energy·Energy
                  + w_genre·Genre + w_mix·Mixability + w_goal·GoalAlignment
                  − w_repeat·RepetitionPenalty ) / Σ(positive Gewichte)
```

Alle Teilbewertungen sind auf `0..1` normalisiert; durch Division durch die Summe
der positiven Gewichte liegt auch `total` in `0..1` — keine willkürlichen Scores.
BPM berücksichtigt Halb-/Doppelzeit; unbekannte Werte ergeben neutral `0.5`
(nicht `0`). Gewichte und Ziel sind konfigurierbar und persistiert.

---

## 5. GUI ↔ Core/Audio (direkte Rust-Aufrufe)

Die native GUI ruft Core und Audio-Engine **direkt** auf — kein IPC. Die
wichtigsten Einstiegspunkte:

| Bereich | Funktion |
|---|---|
| Bibliothek | `MixtreeDb::{list_tracks, get_track, update_track, delete_track}` |
| Import | `music_library::import_folder(db, root, now, progress)` |
| Graph / Pfad | `music_graph::build_graph_with_path`, `pathfinding::recommend_path` |
| Übergang | `transition_engine::score_transition` |
| Empfehlung | `recommendation::next_best_tracks` |
| Analyse | `analysis::worker::run_pending` (Hintergrund-Thread) |
| Beatmatching | `beatmatch::sync_pitch` |
| Audio | `AudioHandle::{send(Command…), position_ms, is_playing, master_peak}` |

---

## 6. Risiken & offene Entscheidungen

1. **Hörbare Audioausgabe auf Linux**: Das CPAL-Backend braucht zum Kompilieren
   `libasound2-dev`. Deshalb ist es feature-gated (`audio-cpal`); der Default-Build
   läuft mit stiller Engine. Die gesamte Engine-Logik ist davon unabhängig getestet.
2. **Echtzeit-Audio**: `Engine::render` ist allokationsfrei, Steuerung über eine
   lock-freie SPSC-Queue — kein Lock/DB/Netzwerk im Audio-Pfad (§5). Umgesetzt.
2. **Analysegenauigkeit (Phase 2)**: BPM/Key/Energy sind Schätzungen. Die
   `analysis`-Traits + `confidence`/`estimated`-Flags erzwingen ehrliche
   Unsicherheitsanzeige; Energie ≠ Lautheit (§7).
4. **Graph-Performance bei großen Bibliotheken (Spec §3C)**: aktuell Top-K-Kanten
   pro Knoten + Score-Schwelle. Für sehr große Libraries folgen Clustering,
   virtuelles Rendering und bedarfsgesteuertes Kantenladen (Phase 6).
5. **Genre-Mapping**: freie Genre-Tags werden heuristisch auf die Taxonomie
   gemappt; unbekannte bleiben leer statt falsch zugeordnet.

---

## 7. Teststrategie (Spec §13)

`mixtree-core` hat 77 Unit-/Integrationstests: Camelot-Kompatibilität,
BPM-Halbzeit, Score-Normalisierung/Reproduzierbarkeit, Pfadfindung
(Dijkstra + Beam, Energieverlauf, Zieltrack), Empfehlungen, Import
(leer, beschädigt, Dubletten, Moved-File, Progress), DB-CRUD/Filter/History,
Job-Queue, die Phase-2-DSP (FFT/Downsample/RMS, WAV-Dekodierung, BPM 120/128/140,
Chroma/Key, Energie-Ordnung, Worker-End-to-End) und ein Integrationstest
(seed → Graph → roter Pfad → Empfehlungen).
Frontend: `tsc --noEmit` und ESLint laufen ohne Fehler/Warnungen.
