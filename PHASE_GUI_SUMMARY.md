# MixTree — Zusammenfassung: Wechsel auf native Rust-GUI (egui)

Stand: 2026-10-28. Format nach Spec §14.

## Hintergrund

Auf Wunsch (Performance; Zweifel an React/TS für Echtzeit-UI; Windows-Ziel) wurde
die Oberfläche vom Web-Prototyp (Tauri 2 + React/TypeScript + Cytoscape) auf eine
**native, GPU-gerenderte Rust-GUI mit egui/eframe** umgestellt. `mixtree-core`
blieb dabei **unverändert** — die saubere Trennung aus Phase 1 hat sich ausgezahlt.

## Implementierte Funktionen

- **Neues Crate `crates/mixtree-gui`** (Binary `mixtree`), cross-platform über
  winit + glow/OpenGL (Linux/Windows/macOS). Nutzt `mixtree-core` **direkt** —
  kein IPC, keine Serialisierung, gemeinsame Rust-Typen.
- **Vollständige UI-Portierung**, nah am Designbild:
  - Header mit Navigation (Tree/Library/Settings), Live-/Demo-Status, aktueller
    BPM/Key/Energie-Anzeige, Mix-Ziel und Uhr.
  - Linke Sidebar: hierarchische Genre-Navigation (einklappbar, farbcodiert),
    Suche, BPM-Filter, Mix-Ziel-Auswahl.
  - **Mix-Baum**: eigenes Painter-Rendering (Hub-and-Spoke), runde genre-farbige
    Track-Knoten mit Labels, Genre-Gruppen als Hubs, **roter Empfehlungspfad** mit
    BPM-Pills, cyan Alternativen, Pfeile; **Schwenken (Drag), Zoom (Rad),
    Klick-Auswahl**.
  - Rechtes Panel: Next-Best-Empfehlung + Alternativen + roter Pfad +
    Übergangs-Tipps.
  - Decks + Mixer: Deck A/B mit Wellenform-Vorschau und BPM-Dial, 3-Band-EQ,
    Crossfader, Mix-Flow-Liste. Transport ist bewusst deaktiviert/markiert
    (Audio folgt) — keine vorgetäuschte Wiedergabe.
  - Bibliothek: Tabelle mit Energiebalken, Analyse-Status, Deck-Laden und
    **Inline-Korrektur** (BPM/Key) + Neu-Analyse.
  - Einstellungen: konfigurierbare Scoring-Gewichte (persistiert).
- **Nativer Ordner-Import** über `rfd` (plattformübergreifender Dialog).
- **Hintergrund-Analyse-Thread**: eigener DB-Connection-Strang, arbeitet die
  Job-Queue ab (Dekodierung + BPM/Key/Energie), meldet Fortschritt per Channel an
  die UI; SQLite im WAL-Modus mit Busy-Timeout für gefahrlosen Parallelzugriff.
- **Verifikation ohne echtes Display**: Die App wird auf einem isolierten
  Xvfb-Display mit Software-GL gerendert und per Screenshot geprüft — ohne den
  echten Desktop zu berühren.

## Geänderte / neue Module

- Neu: `crates/mixtree-gui/src/{main,app,theme}.rs` und
  `views/{header,sidebar,graph,nextbest,decks,library,settings}.rs`.
- `crates/mixtree-core`: `MixtreeDb::init` setzt jetzt WAL + Busy-Timeout
  (sonst unverändert).
- Workspace-`Cargo.toml`: `mixtree-gui` als Default-Member; `src-tauri` nur noch
  explizit baubar.

## Ausgeführte Tests & Prüfungen (alle grün)

```
cargo test  -p mixtree-core                          → 77 passed; 0 failed
cargo clippy -p mixtree-core -p mixtree-gui --all-targets → 0 warnings
cargo fmt                                             → sauber
cargo build (Default: core + gui)                     → erfolgreich
Nativer Start + Screenshot (Xvfb, Software-GL)        → UI rendert korrekt
```

## Bekannte Einschränkungen

- Audiowiedergabe weiterhin offen (eigene Phase): Decks sind Layout + Zustand,
  Wellenformen sind Platzhalter.
- Cover-Art wird erst mit echtem Import/Extraktion angezeigt; sonst Genre-Farbkreise.
- Der Web-/Tauri-Prototyp (`src/`, `src-tauri/`) wird nicht mehr weiterentwickelt,
  bleibt aber als Referenz erhalten.

## Nächste Schritte

- Audio-Engine (CPAL): Wiedergabe, Cue/Loop, echte Wellenformen, EQ/Crossfader an
  den Audiostrom koppeln; Audiothread strikt von DB/GUI entkoppelt (Spec §5).
- Analyse automatisch nach Import starten (Fortschrittsbalken bereits vorbereitet).
- Graph-Skalierung für sehr große Bibliotheken (Clustering, virtuelles Rendering).
