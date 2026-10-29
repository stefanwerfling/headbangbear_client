//! Bottom bar: Deck A, the center mixer (EQ + crossfader), Deck B and Mix-Flow.
//!
//! Transport, EQ and crossfader drive the real audio engine
//! ([`mixtree_audio`]); the playhead reflects the engine's live position.

use crate::app::MixtreeApp;
use crate::theme;
use crate::views::{cap, key_str};
use egui::{Color32, Sense, Stroke, Vec2};
use mixtree_audio::Command as ACmd;
use mixtree_core::models::Track;

pub fn show(app: &mut MixtreeApp, ui: &mut egui::Ui) {
    let full = ui.available_rect_before_wrap();
    let spacing = ui.spacing().item_spacing.x;
    // Top-align the columns so each deck/mixer/flow starts at the same y (a
    // centered layout pushed the shorter columns down).
    ui.horizontal_top(|ui| {
        // Reserve width for the mixer, the Mix-Flow panel and the layout chrome
        // (6 inter-item gaps + 3 separators) so nothing overflows / gets clipped.
        let mixer_w = 150.0;
        let mixflow_w = 240.0;
        let chrome = spacing * 6.0 + 6.0 * 3.0;
        let deck_w = ((full.width() - mixer_w - mixflow_w - chrome) / 2.0).max(220.0);
        deck(ui, app, Side::A, deck_w);
        ui.separator();
        mixer(ui, app);
        ui.separator();
        deck(ui, app, Side::B, deck_w);
        ui.separator();
        // Mix-Flow fills whatever remains, so its right-aligned BPM/energy never
        // falls off the edge.
        mixflow(ui, app);
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    A,
    B,
}

fn deck(ui: &mut egui::Ui, app: &mut MixtreeApp, side: Side, width: f32) {
    let idx = match side {
        Side::A => 0usize,
        Side::B => 1usize,
    };
    let color = if idx == 0 { theme::RED } else { theme::CYAN };
    let track = app.deck(idx).track.clone();
    let loop_active = app.deck(idx).loop_active;
    let pitch = app.deck(idx).pitch;
    let eff_bpm = app.deck(idx).effective_bpm();
    let playing = app.audio.is_playing(idx);
    let pos_ms = app.audio.position_ms(idx);
    let dur_ms = track.as_ref().and_then(|t| t.duration_ms).unwrap_or(0);
    let energy = track.as_ref().and_then(|t| t.energy);
    let cue_pos: Vec<u64> = app.deck(idx).cues.iter().map(|c| c.position_ms).collect();
    // Loop length shown in the LOOP indicator (deck A = 4 bars, deck B = 8 bars,
    // as in the reference).
    let loop_bars: u32 = if idx == 0 { 4 } else { 8 };

    ui.allocate_ui(Vec2::new(width, ui.available_height()), |ui| {
        ui.horizontal_top(|ui| {
            // Left column: title, waveform and transport (flexible width).
            ui.vertical(|ui| {
                ui.set_width(width - 112.0);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                // Cover art: reuse the tree's decoded cover texture when available.
                let (rect, _) = ui.allocate_exact_size(Vec2::new(44.0, 44.0), Sense::hover());
                let tex = track
                    .as_ref()
                    .and_then(|t| app.node_textures.get(&t.id).map(|h| h.id()));
                let rounding = egui::Rounding::same(8.0);
                match tex {
                    Some(id) => {
                        ui.painter().image(
                            id,
                            rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    None => {
                        ui.painter().rect_filled(rect, rounding, theme::PANEL2);
                    }
                }
                ui.painter()
                    .rect_stroke(rect, rounding, Stroke::new(1.0, color));
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(
                            track
                                .as_ref()
                                .map(|t| t.title.as_str())
                                .unwrap_or("Kein Track geladen"),
                        )
                        .size(14.0)
                        .strong(),
                    );
                    let sub = match &track {
                        Some(t) => format!(
                            "{} · {} BPM · {}",
                            cap(t.genre.as_deref()),
                            t.bpm
                                .map(|b| format!("{b:.0}"))
                                .unwrap_or_else(|| "—".into()),
                            key_str(t.key)
                        ),
                        None => format!("Deck {}", if side == Side::A { "A" } else { "B" }),
                    };
                    ui.label(egui::RichText::new(sub).size(11.0).color(theme::TEXT_DIM));
                });
                });

                // Waveform with live playhead.
                ui.add_space(6.0);
                let wf_rect = ui.allocate_space(Vec2::new(width - 128.0, 60.0)).1;
            let peaks = app.deck(idx).waveform.clone();
            let cues = app.deck(idx).cues.clone();
            let sections = app.deck(idx).sections.clone();
            waveform(
                ui,
                wf_rect,
                track.as_ref(),
                &peaks,
                &cues,
                &sections,
                color,
                pos_ms,
                dur_ms,
            );

            // Transport (wired to the audio engine): CUE · skip-prev · play ·
            // skip-next · LOOP · SYNC, matching the reference deck controls.
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let on = track.is_some();
                if ui.add_enabled(on, egui::Button::new("CUE")).clicked() {
                    app.audio_send(ACmd::Cue(idx));
                }
                // Skip to previous cue.
                if ui
                    .add_enabled(on, glyph_btn("⏮", theme::PANEL2))
                    .on_hover_text("Zum vorherigen Cue")
                    .clicked()
                {
                    seek_cue(app, idx, -1, &cue_pos, pos_ms);
                }
                // Primary play/pause — a filled accent-coloured pill.
                let play_txt = if playing { "⏸" } else { "▶" };
                let play = egui::Button::new(
                    egui::RichText::new(play_txt)
                        .size(18.0)
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(if playing { theme::GREEN } else { color })
                .rounding(egui::Rounding::same(6.0))
                .min_size(Vec2::new(58.0, 30.0));
                if ui
                    .add_enabled(on, play)
                    .on_hover_text(if playing { "Pause" } else { "Abspielen" })
                    .clicked()
                {
                    app.audio_send(ACmd::TogglePlay(idx));
                }
                // Skip to next cue.
                if ui
                    .add_enabled(on, glyph_btn("⏭", theme::PANEL2))
                    .on_hover_text("Zum nächsten Cue")
                    .clicked()
                {
                    seek_cue(app, idx, 1, &cue_pos, pos_ms);
                }
                let loop_btn = egui::Button::new("LOOP").fill(if loop_active {
                    theme::ORANGE
                } else {
                    theme::PANEL2
                });
                if ui.add_enabled(on, loop_btn).clicked() {
                    toggle_loop(app, idx, loop_bars);
                }
                let synced = (pitch - 1.0).abs() > 1e-4;
                let sync_btn = egui::Button::new("SYNC").fill(if synced {
                    theme::GREEN
                } else {
                    theme::PANEL2
                });
                if ui
                    .add_enabled(on, sync_btn)
                    .on_hover_text("Tempo an das andere Deck angleichen")
                    .clicked()
                {
                    app.sync_deck(idx);
                }
                });
            });

            // Right column: LOOP indicator, BPM dial and ENERGY bar (fixed width).
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.add_space(8.0);
                loop_box(ui, loop_bars, loop_active, color);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    bpm_dial(ui, eff_bpm, pitch, color);
                    ui.add_space(6.0);
                    energy_bar(ui, energy, color);
                });
            });
        });
    });
}

/// Toggles a loop of `bars` bars at the current playhead for a deck.
fn toggle_loop(app: &mut MixtreeApp, idx: usize, bars: u32) {
    let active = app.deck(idx).loop_active;
    if active {
        app.audio_send(ACmd::SetLoop {
            deck: idx,
            range_ms: None,
        });
        if idx == 0 {
            app.deck_a.loop_active = false
        } else {
            app.deck_b.loop_active = false
        }
    } else {
        let bpm = app
            .deck(idx)
            .track
            .as_ref()
            .and_then(|t| t.bpm)
            .unwrap_or(120.0);
        let start = app.audio.position_ms(idx);
        // bars × 4 beats per bar.
        let len = (bars as f64 * 4.0 * 60_000.0 / bpm) as u64;
        app.audio_send(ACmd::SetLoop {
            deck: idx,
            range_ms: Some((start, start + len)),
        });
        if idx == 0 {
            app.deck_a.loop_active = true
        } else {
            app.deck_b.loop_active = true
        }
    }
}

/// A compact square transport button showing a single glyph.
fn glyph_btn(glyph: &str, fill: Color32) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(glyph).size(14.0).color(theme::TEXT))
        .fill(fill)
        .min_size(Vec2::new(34.0, 30.0))
}

/// Seeks a deck to the next (`dir > 0`) or previous (`dir < 0`) cue point.
fn seek_cue(app: &mut MixtreeApp, idx: usize, dir: i32, cues: &[u64], pos_ms: u64) {
    let mut sorted: Vec<u64> = cues.to_vec();
    sorted.sort_unstable();
    let target = if dir > 0 {
        sorted.into_iter().find(|&p| p > pos_ms + 50)
    } else {
        sorted.into_iter().rev().find(|&p| p + 50 < pos_ms).or(Some(0))
    };
    if let Some(ms) = target {
        app.audio_send(ACmd::Seek { deck: idx, ms });
    }
}

/// A small "LOOP" indicator box showing the loop length in bars.
fn loop_box(ui: &mut egui::Ui, bars: u32, active: bool, color: Color32) {
    let border = if active { theme::ORANGE } else { theme::LINE2 };
    egui::Frame::none()
        .fill(Color32::from_rgb(0x06, 0x0a, 0x12))
        .stroke(Stroke::new(1.0, border))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("LOOP")
                        .size(9.0)
                        .color(theme::TEXT_DIM),
                );
                ui.label(
                    egui::RichText::new(format!("{bars} Bars"))
                        .size(13.0)
                        .strong()
                        .color(if active { theme::ORANGE } else { color }),
                );
            });
        });
}

/// A vertical ENERGY meter for the loaded track (0–100), captioned as in the
/// reference, filled green→orange→red toward the top.
fn energy_bar(ui: &mut egui::Ui, energy: Option<u8>, color: Color32) {
    ui.vertical(|ui| {
        // Extend so the caption never wraps in the narrow right column.
        ui.add(
            egui::Label::new(
                egui::RichText::new("ENERGY")
                    .size(8.0)
                    .color(theme::TEXT_DIM),
            )
            .extend(),
        );
        let (rect, _) = ui.allocate_exact_size(Vec2::new(14.0, 52.0), Sense::hover());
        let p = ui.painter_at(rect);
        p.rect(
            rect,
            egui::Rounding::same(3.0),
            Color32::from_rgb(0x06, 0x0a, 0x12),
            Stroke::new(1.0, theme::LINE),
        );
        let e = energy.unwrap_or(0) as f32 / 100.0;
        let h = e * rect.height();
        let fill = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - h),
            rect.right_bottom(),
        );
        let col = if e > 0.85 {
            theme::RED
        } else if e > 0.6 {
            theme::ORANGE
        } else {
            color
        };
        p.rect_filled(fill, egui::Rounding::same(3.0), col);
        ui.label(
            egui::RichText::new(
                energy
                    .map(|e| format!("{e}%"))
                    .unwrap_or_else(|| "—".into()),
            )
            .size(9.0)
            .color(theme::TEXT_DIM),
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn waveform(
    ui: &egui::Ui,
    rect: egui::Rect,
    track: Option<&Track>,
    peaks: &[f32],
    cues: &[mixtree_core::models::CuePoint],
    sections: &[mixtree_core::models::Section],
    color: Color32,
    pos_ms: u64,
    dur_ms: u64,
) {
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        egui::Rounding::same(8.0),
        Color32::from_rgb(0x06, 0x0a, 0x12),
        Stroke::new(1.0, theme::LINE),
    );
    let frac = if dur_ms > 0 {
        (pos_ms as f32 / dur_ms as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Section tint behind the waveform (detected structure).
    if dur_ms > 0 {
        for s in sections {
            let x0 =
                rect.left() + (s.start_ms as f32 / dur_ms as f32).clamp(0.0, 1.0) * rect.width();
            let x1 = rect.left() + (s.end_ms as f32 / dur_ms as f32).clamp(0.0, 1.0) * rect.width();
            let tint = section_color(s.kind);
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, rect.bottom())),
                egui::Rounding::ZERO,
                Color32::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), 28),
            );
        }
    }
    // Real waveform from the decoded audio's peak envelope.
    if !peaks.is_empty() {
        let bars = peaks.len();
        let bw = rect.width() / bars as f32;
        for (i, &peak) in peaks.iter().enumerate() {
            let h = (peak.max(0.02)) * (rect.height() - 6.0);
            let x = rect.left() + i as f32 * bw;
            let y0 = rect.center().y - h / 2.0;
            let played = (i as f32 / bars as f32) < frac;
            let alpha = if played { 90 } else { 220 };
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(x, y0), Vec2::new((bw * 0.7).max(1.0), h)),
                egui::Rounding::ZERO,
                Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha),
            );
        }
    }
    // Cue markers (suggested + manual): a thin colour line plus a small pill
    // label at the top, as in the reference ("CUE 1/2/3", "Drop", …).
    if dur_ms > 0 {
        // Pill labels are skipped where they would overlap the previous one, so
        // dense cue sets stay legible (the marker line is always drawn).
        let mut last_pill_right = f32::NEG_INFINITY;
        for (i, cue) in cues.iter().enumerate() {
            let cx = rect.left()
                + (cue.position_ms as f32 / dur_ms as f32).clamp(0.0, 1.0) * rect.width();
            let col = cue.color.as_deref().map(theme::hex).unwrap_or(theme::GREEN);
            painter.line_segment(
                [egui::pos2(cx, rect.top()), egui::pos2(cx, rect.bottom())],
                Stroke::new(1.2, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 180)),
            );
            let label = cue
                .label
                .clone()
                .unwrap_or_else(|| format!("CUE {}", i + 1));
            let font = egui::FontId::proportional(7.5);
            let galley = painter.layout_no_wrap(label, font, col);
            let pad = Vec2::new(4.0, 1.5);
            let pw = galley.size().x + pad.x * 2.0;
            let px = cx.clamp(rect.left(), (rect.right() - pw).max(rect.left()));
            if px <= last_pill_right + 2.0 {
                continue; // would collide with the previous pill — line only.
            }
            let pill = egui::Rect::from_min_size(
                egui::pos2(px, rect.top() + 2.0),
                Vec2::new(pw, galley.size().y + pad.y * 2.0),
            );
            painter.rect(
                pill,
                egui::Rounding::same(3.0),
                Color32::from_rgb(0x06, 0x0a, 0x12),
                Stroke::new(1.0, col),
            );
            painter.galley(pill.min + pad, galley, col);
            last_pill_right = pill.right();
        }
    }
    // Playhead.
    if dur_ms > 0 {
        let x = rect.left() + frac * rect.width();
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            Stroke::new(1.5, Color32::WHITE),
        );
    }
    let tag = if track
        .map(|t| t.file_path.starts_with("sample://"))
        .unwrap_or(false)
    {
        "Testton (Beispieldaten)"
    } else {
        "Wellenform-Vorschau"
    };
    // Bottom-right, out of the way of the top cue pills.
    painter.text(
        rect.right_bottom() + Vec2::new(-6.0, -4.0),
        egui::Align2::RIGHT_BOTTOM,
        tag,
        egui::FontId::proportional(8.0),
        theme::TEXT_DIM,
    );
}

/// Color used to tint a structure section behind the waveform.
fn section_color(kind: mixtree_core::models::SectionKind) -> Color32 {
    use mixtree_core::models::SectionKind::*;
    match kind {
        Intro => theme::CYAN,
        Build => theme::ORANGE,
        Drop => theme::RED,
        Breakdown => theme::PURPLE,
        Steady => theme::GREY,
        Outro => theme::CYAN,
        Quiet => theme::LINE2,
    }
}

fn bpm_dial(ui: &mut egui::Ui, bpm: Option<f64>, pitch: f64, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(62.0, 62.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.circle(rect.center(), 30.0, theme::PANEL, Stroke::new(3.0, color));
    p.text(
        rect.center() - Vec2::new(0.0, 8.0),
        egui::Align2::CENTER_CENTER,
        bpm.map(|b| format!("{b:.1}")).unwrap_or_else(|| "—".into()),
        egui::FontId::proportional(15.0),
        theme::TEXT,
    );
    p.text(
        rect.center() + Vec2::new(0.0, 7.0),
        egui::Align2::CENTER_CENTER,
        "BPM",
        egui::FontId::proportional(8.0),
        theme::TEXT_DIM,
    );
    // Pitch adjustment, shown only when non-zero.
    let pct = mixtree_core::beatmatch::pitch_percent(pitch);
    if pct.abs() >= 0.05 {
        p.text(
            rect.center() + Vec2::new(0.0, 17.0),
            egui::Align2::CENTER_CENTER,
            format!("{pct:+.1}%"),
            egui::FontId::proportional(8.0),
            theme::GREEN,
        );
    }
}

fn mixer(ui: &mut egui::Ui, app: &mut MixtreeApp) {
    ui.allocate_ui(Vec2::new(150.0, ui.available_height()), |ui| {
        ui.vertical(|ui| {
            // Keep the mixer a narrow fixed-width column (as in the reference) so
            // it doesn't grow into the decks.
            ui.set_width(150.0);
            // Tight vertical rhythm so the knob stacks + crossfader + Auto-Mix all
            // fit inside the fixed deck-bar height without clipping.
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(6.0);
            let mut a = app.eq_a;
            let mut b = app.eq_b;
            let mut xf = app.crossfader;
            let peak = app.audio.master_peak();
            let (ca, cb) = ui
                .horizontal(|ui| {
                    let ca = eq_column(ui, "A", &mut a, theme::RED);
                    ui.add_space(6.0);
                    vu_meter(ui, peak);
                    ui.add_space(6.0);
                    let cb = eq_column(ui, "B", &mut b, theme::CYAN);
                    (ca, cb)
                })
                .inner;
            ui.add_space(6.0);
            let xchanged = ui
                .add(egui::Slider::new(&mut xf, 0.0..=1.0).show_value(false))
                .changed();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("A").size(9.0).color(theme::TEXT_DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("B").size(9.0).color(theme::TEXT_DIM));
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("Crossfader")
                                .size(9.0)
                                .color(theme::TEXT_DIM),
                        );
                    });
                });
            });

            if ca {
                app.eq_a = a;
                app.audio_send(ACmd::SetEq { deck: 0, gains: a });
            }
            if cb {
                app.eq_b = b;
                app.audio_send(ACmd::SetEq { deck: 1, gains: b });
            }
            if xchanged {
                app.crossfader = xf;
                app.audio_send(ACmd::SetCrossfader(xf));
            }

            ui.add_space(4.0);
            let auto = egui::Button::new(egui::RichText::new("Auto-Mix A ▶ B").size(10.0))
                .fill(theme::PANEL2)
                .min_size(Vec2::new(ui.available_width(), 22.0));
            if ui
                .add_enabled(app.deck_b.track.is_some(), auto)
                .on_hover_text("Automatischer Übergang über 16 Takte")
                .clicked()
            {
                app.auto_mix();
            }
        });
    });
}

/// EQ column. `eq` is `[low, mid, high]`; the knobs are shown HIGH→LOW to match
/// a physical mixer. Returns `true` if any band changed. Drag a knob vertically
/// to adjust; double-click recentres it.
fn eq_column(ui: &mut egui::Ui, label: &str, eq: &mut [f32; 3], color: Color32) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(label).size(10.0).color(theme::TEXT_DIM));
            for (name, i) in [("HIGH", 2usize), ("MID", 1), ("LOW", 0)] {
                changed |= knob(ui, &mut eq[i], color);
                ui.label(egui::RichText::new(name).size(8.0).color(theme::TEXT_DIM));
            }
        });
    });
    changed
}

/// A rotary knob for a normalised `0..=1` value. Drag vertically to change;
/// double-click recentres to 0.5. Returns `true` on change.
fn knob(ui: &mut egui::Ui, value: &mut f32, color: Color32) -> bool {
    let size = 26.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(size, size), Sense::click_and_drag());
    let mut changed = false;
    if resp.dragged() {
        *value = (*value - resp.drag_delta().y * 0.012).clamp(0.0, 1.0);
        changed = true;
    }
    if resp.double_clicked() {
        *value = 0.5;
        changed = true;
    }
    let p = ui.painter_at(rect);
    let c = rect.center();
    let r = size * 0.42;
    p.circle_filled(c, r, theme::PANEL2);
    p.circle_stroke(c, r, Stroke::new(1.5, theme::LINE2));
    // Value pointer (knob sweeps −135°…+135° from 12 o'clock).
    let theta = (-135.0 + *value * 270.0_f32).to_radians();
    let dir = Vec2::new(theta.sin(), -theta.cos());
    p.line_segment(
        [c + dir * (r * 0.3), c + dir * (r * 0.95)],
        Stroke::new(2.5, color),
    );
    changed
}

/// Master VU meter: a vertical bar whose fill tracks the engine's output peak,
/// green → orange → red toward clipping.
fn vu_meter(ui: &mut egui::Ui, peak: f32) {
    ui.vertical(|ui| {
        // Headphone (cue-monitor) glyph above the meter, as in the reference.
        let (hp, _) = ui.allocate_exact_size(Vec2::new(16.0, 14.0), Sense::hover());
        headphone_icon(ui.painter(), hp, theme::TEXT_DIM);
        ui.label(egui::RichText::new("VU").size(10.0).color(theme::TEXT_DIM));
        let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 84.0), Sense::hover());
        let p = ui.painter_at(rect);
        p.rect(
            rect,
            egui::Rounding::same(2.0),
            Color32::from_rgb(0x06, 0x0a, 0x12),
            Stroke::new(1.0, theme::LINE),
        );
        let level = peak.clamp(0.0, 1.0);
        let h = level * rect.height();
        let fill = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - h),
            rect.right_bottom(),
        );
        let col = if level > 0.9 {
            theme::RED
        } else if level > 0.6 {
            theme::ORANGE
        } else {
            theme::GREEN
        };
        p.rect_filled(fill, egui::Rounding::same(2.0), col);
    });
}

/// A simple over-ear headphone glyph: a headband arc plus two ear cups.
fn headphone_icon(p: &egui::Painter, rect: egui::Rect, col: Color32) {
    let c = egui::pos2(rect.center().x, rect.bottom() - 2.0);
    let r = rect.width() * 0.42;
    // Headband (upper half arc).
    let n = 12;
    let pts: Vec<egui::Pos2> = (0..=n)
        .map(|i| {
            let a = std::f32::consts::PI + i as f32 / n as f32 * std::f32::consts::PI;
            egui::pos2(c.x + a.cos() * r, c.y + a.sin() * r)
        })
        .collect();
    for w in pts.windows(2) {
        p.line_segment([w[0], w[1]], Stroke::new(1.6, col));
    }
    // Ear cups.
    let cup = Vec2::new(3.0, 5.0);
    for sx in [-1.0f32, 1.0] {
        let cx = c.x + sx * r;
        p.rect_filled(
            egui::Rect::from_center_size(egui::pos2(cx, c.y), cup * 2.0),
            egui::Rounding::same(1.5),
            col,
        );
    }
}

fn mixflow(ui: &mut egui::Ui, app: &mut MixtreeApp) {
    let w = ui.available_width().max(180.0);
    ui.allocate_ui(Vec2::new(w, ui.available_height()), |ui| {
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Mix-Flow")
                    .size(12.0)
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(4.0);

            // (id, title, genre, bpm, energy, is_current).
            type FlowRow = (i64, String, Option<String>, Option<f64>, Option<u8>, bool);
            let mut rows: Vec<FlowRow> = Vec::new();
            if let Some(t) = &app.deck_a.track {
                rows.push((
                    t.id,
                    t.title.clone(),
                    t.genre.clone(),
                    t.bpm,
                    t.energy,
                    true,
                ));
            }
            for r in app.recs.iter().take(3) {
                rows.push((
                    r.track_id,
                    r.title.clone(),
                    r.genre.clone(),
                    r.bpm,
                    r.energy,
                    false,
                ));
            }

            for (id, title, genre, bpm, energy, active) in rows {
                let resp = egui::Frame::none()
                    .fill(theme::PANEL2)
                    .stroke(Stroke::new(
                        1.0,
                        if active { theme::RED } else { theme::LINE },
                    ))
                    .rounding(egui::Rounding::same(7.0))
                    .inner_margin(egui::Margin::symmetric(9.0, 6.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (r, _) =
                                ui.allocate_exact_size(Vec2::new(8.0, 8.0), Sense::hover());
                            ui.painter().circle_filled(
                                r.center(),
                                4.0,
                                dot_color(genre.as_deref()),
                            );
                            ui.label(egui::RichText::new(title).size(12.0));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} · {}%",
                                            bpm.map(|b| format!("{b:.0}"))
                                                .unwrap_or_else(|| "—".into()),
                                            energy
                                                .map(|e| e.to_string())
                                                .unwrap_or_else(|| "—".into()),
                                        ))
                                        .size(9.0)
                                        .color(theme::TEXT_DIM),
                                    );
                                },
                            );
                        });
                    })
                    .response;
                if resp.interact(Sense::click()).clicked() {
                    app.set_current(id);
                }
            }
            if app.deck_a.track.is_none() && app.recs.is_empty() {
                ui.label(
                    egui::RichText::new("Noch kein Flow — Track im Baum wählen.")
                        .size(10.0)
                        .color(theme::TEXT_DIM),
                );
            }
        });
    });
}

fn dot_color(genre: Option<&str>) -> Color32 {
    match genre {
        Some("techno") => theme::RED,
        Some("house") => theme::CYAN,
        Some("trance") => theme::GREEN,
        Some("ambient") => theme::PURPLE,
        Some("hardstyle") => theme::ORANGE,
        _ => theme::GREY,
    }
}
