# MixTree — Zusammenfassung: Pixelnaher Mix-Baum, Plasma-Pfad, Beat-Ring

Stand: 2026-10-28. Native Rust-GUI (egui/eframe) — **keine** Webanwendung, kein WebView.

## Geänderte / neue Dateien

- **Neu** `crates/mixtree-gui/src/views/tree_layout.rs`: deterministisches
  Fixed-Layout. Logischer **viewBox** `VIEW_W=860 × VIEH_H=600`, explizite
  Hub-Positionen (`hub_pos`) und Sample-Track-Positionen (`sample_track_pos`) aus
  dem Referenzbild. Keine Force-/Zufallslayouts. Tests: alle Knoten im viewBox,
  Determinismus.
- `crates/mixtree-gui/src/views/graph.rs`:
  - **Fixed-viewBox-Transform** (eine proportionale Abbildung logisch→Screen;
    Fenster-Resize skaliert alles, verteilt nichts neu; Pan/Zoom additiv).
  - **Layer-Reihenfolge**: Hintergrund → Genre-Äste (kubische Bézier, genre-farbig)
    → Alternativen (cyan) → **roter Plasma-Pfad** → Knoten (Hubs als Glow-Pills
    mit Icon, Tracks als neon-umrandete Scheiben + Label) → **Beat-Ring** →
    BPM-Pills.
  - `draw_plasma`: Glow-Kontur + heller Kern + dezenter Puls (`sin(time)`) +
    **wandernde Lichtmarke** (Komet) in Wiedergabereihenfolge entlang der
    gesampelten Pfadkurve. Geometrie bleibt fix.
  - `draw_beat_ring`: konzentrischer Ring + **umlaufende Lichtmarke**; Helligkeit
    pulsiert zum Beat. Der Knoten bewegt sich nicht.
  - `request_repaint_after(33ms)` nur bei aktiver Animation (kein Busy-Loop bei
    reduzierter Bewegung).
- `crates/mixtree-gui/src/app.rs`: `reduced_motion`-Flag; env-gesteuerter
  `MIXTREE_AUTOPLAY`-Dev-Hook (nur für Screenshots, Standard aus).
- `crates/mixtree-gui/src/views/settings.rs`: Umschalter „Reduzierte Bewegung".

## Rendering-Architektur

`egui::Painter` auf einem eigenen Zeichenbereich. Statische Geometrie
(`layout_positions`) ist von den Animationswerten getrennt; BPM, Energie,
Playhead oder Track-Wechsel verändern die Knotenpositionen nicht. HiDPI/Resize
über die eine proportionale viewBox-Transformation.

## Beat-Synchronisation

Quelle ist die **echte Playback-Position** der Audio-Engine, kein freier Timer:
`phase = ((position_s) / (60/bpm)).rem_euclid(1.0)`, mit
`position_s = AudioHandle::position_ms(0)/1000` und `bpm = Deck-A effektive BPM`
(inkl. Pitch). `playing = AudioHandle::is_playing(0)`. Zustände: Wiedergabe
(Ring+Umlauf aktiv), Pause (ruhender Ring), Seek (Phase folgt der neuen Position,
da aus ihr berechnet), Track-Wechsel (Ring wandert zum Deck-A-Knoten),
`noBeatgrid` (BPM-basiert, da aktuell kein Beat-Offset persistiert wird),
`reduced-motion` (statisch). Beat-Offset = 0, bis ein persistiertes Beatgrid
vorliegt.

## Tatsächlich ausgeführte visuelle Tests

- **Statischer Baum**: `scratchpad/gui-fixed.png`.
- **Plasma-Pfad + Beat-Ring (Wiedergabe)**: `scratchpad/play1.png`, `play2.png`
  (via `MIXTREE_AUTOPLAY=1` auf isoliertem Xvfb + Software-GL).
- **Animations-Nachweis**: `compare -metric AE play1 play2` → ~3114 Pixel
  Unterschied (Lichtmarke/Beat bewegt sich, Geometrie stabil).
- **Overlay/Pixel-Diff gegen Referenz**: Baumbereiche aus Referenz und Render
  zugeschnitten, auf gleiche Größe skaliert, 50/50-Overlay (`overlay_tree.png`).
  Ergebnis: Knotenpositionen liegen nah an der Referenz, gleiche Topologie.
- `cargo fmt --check` ✓ · `cargo clippy --all-targets` → 0 Warnungen ·
  `cargo test` → core 88, audio 18, gui 2 (alle grün).

## Ehrliche Einschränkung zur „Pixelgenauigkeit"

Es ist **keine** Pixel-für-Pixel-Identität mit dem Referenzbild — und ich behaupte
das auch nicht. Der Entwurf ist ein gestaltetes Mockup mit echten **Cover-Fotos**
in den Knoten und weichem Gauß-**Glow**; egui hat keinen Blur (Glow wird mit
gestapelten Konturen angenähert) und es liegen keine Cover-Bilddateien vor
(dunkle Scheiben mit Genre-Glow als Platzhalter). **Übereinstimmend**: Anordnung/
Topologie, Hub-Pills mit Icon + BPM-Range, neon-umrandete Track-Knoten mit
Titel/BPM/Energie, kurvige gerichtete Äste, roter Plasma-Pfad mit wanderndem
Licht, beat-synchroner Ring. Der Overlay-Vergleich dokumentiert die Nähe.
