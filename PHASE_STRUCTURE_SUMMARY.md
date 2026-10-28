# MixTree — Zusammenfassung: Songstruktur & Cue-Vorschläge (Spec §6/§7/§8)

Stand: 2026-10-28. Format nach Spec §14.

## Implementierte Funktionen

- **Struktur-/Drop-Erkennung** (`mixtree-core::structure_detection`): segmentiert
  einen Track anhand einer Kurzzeit-Energiekurve in Abschnitte
  **Intro / Build / Drop / Breakdown / Steady / Outro / Quiet**. Transparent und
  signalbasiert (kein ML nötig); jeder Abschnitt trägt eine **Confidence** und ist
  ausdrücklich eine Schätzung (Spec §6: Unsicherheit zeigen statt falscher
  Präzision). Gleichmäßige/zu kurze Tracks ergeben einen einzelnen Abschnitt.
- **Cue-Vorschläge** (`suggest_cues`): „Mix In" am Intro-Ende, ein Cue an jedem
  Drop, „Mix Out" am Outro-Anfang — als *structural* markiert (getrennt von
  manuellen Hot-Cues).
- **Persistenz**: Migration **v2** mit `sections`-Tabelle; Repo-Methoden
  `replace_sections` / `list_sections` und `add_cue` / `list_cues` /
  `clear_structural_cues` (Hot-Cues bleiben erhalten).
- **Analyse-Worker**: `AnalysisKind::Structure` führt jetzt die Erkennung aus,
  speichert Abschnitte versioniert und ersetzt die vorgeschlagenen Cues. Import
  und „Neu analysieren" reihen Struktur-Jobs mit ein.
- **GUI**: Die Deck-Wellenform zeigt die **erkannten Abschnitte** als farbige
  Hintergrund-Tönung (genre-nahe Struktur-Farben) und **Cue-Marker** als farbige
  Linien mit Label (z. B. „Drop 1", „Mix In") — zusätzlich zum Live-Playhead.

## Geänderte / neue Module

- Neu: `crates/mixtree-core/src/structure_detection.rs`; Modelle `Section`,
  `SectionKind`, `CuePoint` (in `models.rs`); Migration v2 + Repo-Methoden
  (`database/mod.rs`, `database/schema.rs`); Worker-Zweig (`analysis/worker.rs`);
  Import-Enqueue (`music_library.rs`).
- GUI: `Deck { cues, sections }` + Laden in `load_deck_audio`; Wellenform zeichnet
  Abschnitts-Tönung + Cue-Marker (`views/decks.rs`); Struktur-Job in „Neu
  analysieren" (`views/library.rs`).

## Ausgeführte Tests & Prüfungen (alle grün)

```
cargo test  -p mixtree-core   → 88 passed  (inkl. Struktur + Cues)
cargo test  -p mixtree-audio  → 16 passed
cargo clippy --all-targets    → 0 warnings
cargo fmt / cargo build        → sauber / erfolgreich
Nativer Start + Screenshot     → keine Regression
```

Struktur-Tests prüfen u. a.: leeres/uniformes Signal, ein synthetisches
Intro→Drop→Breakdown→Drop→Outro (korrekte Kennzeichnung, lückenlose Abdeckung)
sowie die Cue-Vorschläge (Mix In / Drop / Mix Out, als structural markiert).

## Bekannte Einschränkungen / Nächste Schritte

- Erkennung über Energie-Schwellen — robust für klare Drops, bei subtilem
  Material ungenauer; Confidence wird mitgeführt. Beat-Grid-Snap der Grenzen
  (an die Beats aus Phase 2) ist ein sinnvoller nächster Schritt.
- Vocal-Erkennung / Vocal-Konflikte (Spec §6) noch offen.
- **Geplante Übergänge** (Spec §6): die Cue-Vorschläge sind die Grundlage; die
  automatisierte EQ-/Volume-Fahrt über N Takte baut darauf auf.
