# MixTree — Zusammenfassung: Mixing-Feinschliff (Spec §5/§7)

Stand: 2026-10-28. Format nach Spec §14. Baut auf [`PHASE_AUDIO_SUMMARY.md`](PHASE_AUDIO_SUMMARY.md).

## Implementierte Funktionen

- **Beatmatching / Sync** (`mixtree-core::beatmatch`): oktav-bewusste
  Tempo-Angleichung. `sync_pitch(master, deck)` wählt unter den
  Oktav-Äquivalenten (×0.5 / ×1 / ×2) die **kleinste Tempo-Änderung** und clamped
  auf den Engine-Bereich `0.5..=2.0`. Dazu `effective_bpm` und `pitch_percent`.
  - GUI: **SYNC-Button** pro Deck gleicht das Tempo ans andere Deck an; das
    BPM-Dial zeigt die **effektive BPM** und die **Pitch-Prozent** an.
- **Echte Wellenform** (`mixtree-audio::TrackAudio::peaks`): normalisierte
  Peak-Hüllkurve aus den dekodierten Samples. Die Decks zeichnen diese echte
  Hüllkurve (statt Pseudo-Balken); der gespielte Teil wird abgedunkelt, der
  Playhead folgt live.
- **Master-VU-Meter**: die Engine veröffentlicht den Block-Spitzenpegel lock-frei
  (`AudioHandle::master_peak`); der Mixer zeigt ein vertikales VU (grün→orange→rot)
  zwischen den EQ-Spalten.

## Geänderte / neue Module

- Neu: `crates/mixtree-core/src/beatmatch.rs` (+ in `lib.rs` registriert).
- `crates/mixtree-audio`: `TrackAudio::peaks`, `Shared::master_peak` + `block_peak`,
  `AudioHandle::master_peak`; Peak-Publishing in Null-/Pipe-/CPAL-Pfad.
- `crates/mixtree-gui`: `Deck { pitch, waveform }`, `set_pitch`/`sync_deck`;
  Decks: SYNC-Button, effektive-BPM/Pitch-Anzeige, echte Wellenform, VU-Meter.

## Aufräumen

- Der **Web-/Tauri-Prototyp wurde entfernt** (`src/`, `src-tauri/`, `package.json`,
  Vite/TS/ESLint-Dateien, `node_modules/`). Das Projekt ist jetzt reines Rust;
  Workspace-Member sind nur noch die drei `crates/*`.

## Ausgeführte Tests & Prüfungen (alle grün)

```
cargo test  -p mixtree-core   → 84 passed  (inkl. 7 beatmatch)
cargo test  -p mixtree-audio  → 16 passed  (inkl. peaks)
cargo clippy --all-targets    → 0 warnings
cargo fmt / cargo build        → sauber / erfolgreich
Nativer Start + Screenshot (Xvfb) → echte Wellenform, SYNC, VU sichtbar
```

## Bekannte Einschränkungen / Nächste Schritte

- Sync gleicht aktuell nur das **Tempo** an (kein Phasen-/Beat-Grid-Alignment der
  Downbeats, kein automatisches Nudging) — nächster Schritt mit dem Beat-Grid aus
  Phase 2.
- Pitch weiterhin varispeed (Tempo ändert Tonhöhe); echter Time-Stretch folgt.
- VU ist ein einfacher Spitzenpegel (kein RMS/Peak-Hold, kein Clip-Limiter).
