# MixTree — Zusammenfassung Phase 1 (Grundgerüst)

Stand: 2026-10-28. Format nach Spec §14.

## Implementierte Funktionen

- **Projektgerüst**: Cargo-Workspace mit `mixtree-core` (Logik) und `src-tauri`
  (Tauri-2-Shell) + React/TypeScript/Vite/Cytoscape-Frontend.
- **SQLite-Bibliothek** (eingebettet, `bundled`): versioniertes Schema mit
  Migrationen, Tabellen für Tracks, Genres, Cue-Points, Loops, Playlists,
  Mix-History und Analyse-Job-Queue. `analysis_version` für gezielte
  Neuberechnung. Daten bleiben nach Neustart erhalten (persistente Datei im
  App-Data-Verzeichnis).
- **Musikimport**: rekursiver Ordner-Scan, Metadaten via `lofty`
  (Titel/Künstler/Album/Genre/Jahr/Dauer/Cover-Flag), Content-Hash für
  Dubletten- und Moved-File-Erkennung, Fortschritts-Event, robuste
  Fehlerbehandlung (beschädigte/nicht unterstützte Dateien brechen den Import
  nicht ab). Formate: MP3, FLAC, WAV, OGG, M4A, AAC.
- **Bibliotheksansicht**: sortierbare Tabelle (Titel/Künstler/Genre/BPM/Key/
  Energie/Status) mit Energiebalken und Deck-A/B-Laden; Suche + Genre- + BPM-
  Filter in der linken Sidebar.
- **Interaktiver Musikbaum** (Cytoscape): Genre-Hubs + Track-Knoten in
  Hub-and-Spoke-Layout, genre-farbige Knoten, bewertete Übergangskanten,
  **roter Empfehlungspfad**, cyan/blaue Alternativen, Zoom/Pan/Drag,
  Klick = in Deck A laden, Rechtsklick = in Deck B.
- **Transition-Engine** (Vorgriff auf Phase 3, da für den Graphen nötig):
  normalisiertes, mehrdimensionales Scoring (BPM inkl. Halbzeit, Harmonik via
  Camelot, Energie, Genre, Mixability, Ziel, Wiederholungsstrafe) mit
  erklärenden Notizen.
- **Pfadfindung**: Dijkstra (zum Zieltrack) + zielgerichtete Beam-Suche für den
  roten Pfad; sieben Mix-Ziele konfigurierbar.
- **Next-Best-Track-Panel**: empfohlener nächster Track + Alternativen +
  BPM-Differenz + Übergangsqualität + Begründung + (als Schätzung markierte)
  Cue-/Loop-/Dauer-Vorschläge.
- **Decks & Mixer (Layout)**: zwei Decks mit Wellenform-Vorschau, BPM-Dial,
  3-Band-EQ-Reglern und Crossfader; Mix-Flow-Liste. Transport (Play/Cue/Loop)
  ist bewusst deaktiviert und als „Audio-Engine ab Phase 4" markiert — keine
  vorgetäuschte Audiofunktion.
- **Phase-2-Schnittstellen**: `BpmDetector`/`KeyDetector`/`EnergyAnalyzer`-Traits
  mit `confidence`/`estimated`-Feldern und persistente Analyse-Job-Queue; Import
  reiht fehlende Features automatisch ein.
- **Tauri-API**: 16 Commands (Import, Bibliothek, Graph, Pfad, Transition,
  Empfehlung, Konfiguration, Job-Status) + `import://progress`-Event.

## Geänderte / neue Module

- `crates/mixtree-core/src/`: `models`, `camelot`, `configuration`, `error`,
  `metadata`, `music_library`, `transition_engine`, `music_graph`, `pathfinding`,
  `recommendation`, `sample_data`, `database/{mod,schema}`,
  `analysis/{mod,job_queue}`, `lib`.
- `src-tauri/src/`: `main`, `lib`, `state`, `commands/{mod,library,graph,
  recommendation,config}`; `tauri.conf.json`, `capabilities/default.json`,
  `build.rs`, generierte Icons.
- `src/`: `main.tsx`, `App.tsx`, `styles.css`, `lib/{types,api,mock}.ts`,
  `components/{Header,GenreSidebar,MixTreeGraph,NextBestPanel,DeckBar,
  LibraryView}.tsx`.
- Konfiguration: `Cargo.toml` (Workspace), `package.json`, `vite.config.ts`,
  `tsconfig*.json`, `.eslintrc.cjs`, `index.html`, `.gitignore`,
  `ARCHITECTURE.md`.

## Ausgeführte Tests & statische Prüfungen

```
cargo test  -p mixtree-core            → 54 passed; 0 failed
cargo clippy -p mixtree-core --all-targets → 0 warnings
cargo fmt   -p mixtree-core --check     → sauber
npm run typecheck (tsc --noEmit)        → 0 Fehler
npm run lint (eslint, --max-warnings 0) → 0 Fehler/Warnungen
npm run build (vite)                     → erfolgreich
```

Abgedeckt u. a.: Camelot-Parsing/-Kompatibilität, BPM-Halbzeit,
Score-Normalisierung & Reproduzierbarkeit, Wiederholungsstrafe, Mix-Ziele,
Dijkstra/Beam-Pfade, Empfehlungssortierung, Import (leer/beschädigt/Dublette/
Moved-File/Progress), DB-CRUD & Filter, Mix-History, Job-Queue sowie ein
End-to-End-Test (seed → Graph+roter Pfad → Empfehlungen).

## Testergebnisse

Alle oben genannten Prüfungen laufen grün. Das Frontend wurde im Browser gegen
den Beispieldaten-Fallback verifiziert (Tree-Graph mit rotem Pfad,
Next-Best-Panel mit echter berechneter Empfehlung, Bibliothekstabelle, Decks &
Mixer).

## Bekannte Einschränkungen

- **Desktop-Build hier nicht möglich**: `libwebkit2gtk-4.1-dev` (und für Phase 4
  `libasound2-dev`) fehlen auf der Maschine, kein passwortloses `sudo`. Die
  Tauri-Befehlsschicht ist geschrieben, aber nicht auf dieser Maschine
  kompiliert (Fehler liegt rein in `javascriptcore-rs-sys`/pkg-config). Siehe
  `ARCHITECTURE.md` §2.
- **Keine echte Audiowiedergabe** (Phase 4). Decks sind Layout + Zustand;
  Wellenformen sind Platzhalter-Vorschauen.
- **Keine echte Signalanalyse** (Phase 2). BPM/Key/Energie stammen aus Tags oder
  Beispieldaten; die Job-Queue ist angelegt, der DSP-Worker folgt.
- **Browser-Fallback** nutzt eine TypeScript-Spiegelung der Scoring-Logik — nur
  für Entwicklung/Screenshots; die echte Berechnung macht der Rust-Core.
- Graph-Skalierung (Clustering/virtuelles Rendering) für sehr große Bibliotheken
  ist auf Phase 6 terminiert.

## Nächste konkrete Arbeitsschritte (Phase 2)

1. Audio-Dekodierung mit `symphonia` → PCM-Puffer für Analyse.
2. `BpmDetector` implementieren (Onset/Autokorrelation), Beatgrid speichern,
   Halb-/Doppelzeit-Kandidaten + manuelle Korrektur.
3. `KeyDetector` (Chroma/Krumhansl) → Camelot, mit Confidence.
4. `EnergyAnalyzer` (RMS, spektraler Schwerpunkt, Transienten) → `0–100`,
   klar als Schätzung.
5. Hintergrund-Worker, der die Analyse-Job-Queue abarbeitet und Ergebnisse
   versioniert in die DB schreibt; Fortschritt per Event an die UI.
6. UI: Analyse-Status/-Fortschritt im Header, manuelle BPM-/Key-Korrektur in der
   Bibliothek.
