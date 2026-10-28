# MixTree — Zusammenfassung: Audio-Engine (Phase 4)

Stand: 2026-10-28. Format nach Spec §14.

## Hintergrund / Einschränkung

CPAL braucht auf Linux `libasound2-dev` (fehlt hier, kein sudo), lässt sich hier
also nicht kompilieren. Die Audio-Engine ist deshalb als **reines, voll
getestetes Rust-Crate** gebaut; die Geräteausgabe ist über **austauschbare
Backends** gekapselt. So ist die gesamte Misch-/DSP-Logik hier verifizierbar,
und echte Ausgabe ist ein Backend-Wechsel entfernt.

## Implementierte Funktionen

- **Neues Crate `crates/mixtree-audio`** — Echtzeit-Mischengine, geräteunabhängig:
  - **Zwei Decks** mit unabhängiger Wiedergabe, Position (sample-genau),
    Play/Pause, **Cue** (setzen & anspringen) und **Beat-Loops**.
  - **Equal-Power-Crossfader** (A↔B) + Master-Gain.
  - **3-Band-EQ** pro Deck (RBJ-Biquads: Low-Shelf/Peak/High-Shelf), DJ-typisch
    mit Kill bei 0 und Unity bei 0.5.
  - **Resampling / Pitch-Tempo** über lineare Interpolation (Rate-Anpassung +
    Pitch-Faktor), Quell-/Ausgabe-Rate werden angeglichen.
  - **Lock-freie Steuerung**: Kommandos fließen über eine SPSC-Queue (`rtrb`) zum
    Audio-Thread; `Engine::render` ist allokationsfrei (Spec §5).
  - **Backends**: `NullBackend` (still, echtzeit-getaktet — sicherer Default,
    beschallt die Maschine nicht), `PipeBackend` (`pacat`, Linux),
    **CPAL-Backend** hinter Feature `cpal` (cross-platform, inkl. Windows/macOS).
- **Core-Erweiterung**: `decode::decode_interleaved` — Stereo-Dekodierung bei
  nativer Rate für die Wiedergabe (ergänzt die Mono-Analyse-Dekodierung).
- **GUI-Integration** (native egui-App):
  - Decks laden beim Auswählen/Empfehlen echtes Audio; für die `sample://`
    Beispieldaten (keine Datei) wird ein klar markierter **Testton** erzeugt, damit
    Transport/Playhead/Mixer vorführbar sind.
  - **CUE / Play-Pause / LOOP** sind jetzt echte Steuerungen (senden Engine-
    Kommandos); der **Playhead** auf der Wellenform folgt der Live-Position.
  - **3-Band-EQ-Fader** und **Crossfader** wirken live auf die Engine.
  - Standardmäßig läuft der stille `NullBackend` (kein Ton auf die Lautsprecher
    des Nutzers); echte Ausgabe über `--features audio-cpal` oder den Pipe-Backend.

## Geänderte / neue Module

- Neu: `crates/mixtree-audio/src/{lib,engine,eq,backend,command}.rs`.
- `crates/mixtree-core/src/analysis/decode.rs`: `decode_interleaved` + `InterleavedAudio`.
- `crates/mixtree-gui`: Audio-Handle in `app.rs`, Transport/EQ/Crossfader/Loop +
  Playhead in `views/decks.rs`; Feature `audio-cpal`.
- Workspace: `mixtree-audio` als Default-Member.

## Ausgeführte Tests & Prüfungen (alle grün)

```
cargo test  -p mixtree-audio                 → 14 passed
cargo test  -p mixtree-core                  → 77 passed
cargo clippy (core + audio + gui) --all-targets → 0 warnings
cargo fmt                                     → sauber
cargo build (Default: core + audio + gui)     → erfolgreich
Nativer Start + Screenshot (Xvfb)             → Transport/Playhead/Mixer sichtbar
```

Audio-Tests decken ab: Stille ohne Play; Wiedergabe + Positionsfortschritt;
Crossfader-Auswahl (A/B, Stummschaltung der Gegenseite); Gain-0-Stille;
Pitch (2× schnelleres Fortschreiten); **Loop-Wrap**; Stopp am Track-Ende;
Cue-Rücksprung; EQ (neutral transparent, Low-Kill dämpft Bass, High-Boost hebt
Höhen, Mapping-Endpunkte); Live-Handle (Load+Play über den Audio-Thread lässt den
Playhead vorrücken).

## Bekannte Einschränkungen

- Hörbare Ausgabe hier bewusst deaktiviert (stiller Default), um die Maschine des
  Nutzers nicht zu beschallen; verifiziert wird über Tests + die render-Logik.
  Echte Ausgabe: `cargo run -p mixtree-gui --features audio-cpal` (benötigt
  `libasound2-dev` auf Linux) bzw. auf Windows/macOS.
- Pitch/Tempo per linearer Interpolation (varispeed) — noch kein unabhängiger
  Time-Stretch (tempo ohne Tonhöhenänderung) und kein Auto-Beatmatch-Sync.
- EQ-Bänder mit festen Frequenzen (200/1000/4000 Hz); kein Limiter/Clip-Schutz
  am Master.

## Nächste Schritte

- Beatmatching/Sync: Deck-BPM an das Beat-Grid (Phase 2) koppeln, Pitch
  automatisch angleichen, Phasen-Nudge.
- Time-Stretch (z. B. WSOLA/Phase-Vocoder) für Tempo ohne Pitch-Shift.
- Geplante Übergänge (Spec §6): Cue-/Loop-Vorschläge + automatisierte EQ-/Volume-
  Fahrten über einen definierten Zeitraum.
- Echte Wellenform aus den dekodierten Samples (statt Platzhalter) + Master-VU.
