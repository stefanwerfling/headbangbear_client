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
    ui.horizontal(|ui| {
        let deck_w = (full.width() - 150.0 - 240.0 - 24.0) / 2.0;
        deck(ui, app, Side::A, deck_w);
        ui.separator();
        mixer(ui, app);
        ui.separator();
        deck(ui, app, Side::B, deck_w);
        ui.separator();
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

    ui.allocate_ui(Vec2::new(width, ui.available_height()), |ui| {
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                // Cover placeholder.
                let (rect, _) = ui.allocate_exact_size(Vec2::new(44.0, 44.0), Sense::hover());
                ui.painter().rect(
                    rect,
                    egui::Rounding::same(8.0),
                    theme::PANEL2,
                    Stroke::new(1.0, color),
                );
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
            let wf_rect = ui.allocate_space(Vec2::new(width - 16.0, 60.0)).1;
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

            // Transport (now wired to the audio engine).
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                if ui
                    .add_enabled(track.is_some(), egui::Button::new("CUE"))
                    .clicked()
                {
                    app.audio_send(ACmd::Cue(idx));
                }
                // Primary transport control — made deliberately prominent so it
                // reads as "the play button": a filled accent-coloured pill with
                // a clear ▶ / ⏸ glyph.
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
                    .add_enabled(track.is_some(), play)
                    .on_hover_text(if playing { "Pause" } else { "Abspielen" })
                    .clicked()
                {
                    app.audio_send(ACmd::TogglePlay(idx));
                }
                let loop_btn = egui::Button::new("LOOP").fill(if loop_active {
                    theme::ORANGE
                } else {
                    theme::PANEL2
                });
                if ui.add_enabled(track.is_some(), loop_btn).clicked() {
                    toggle_loop(app, idx);
                }
                let synced = (pitch - 1.0).abs() > 1e-4;
                let sync_btn = egui::Button::new("SYNC").fill(if synced {
                    theme::GREEN
                } else {
                    theme::PANEL2
                });
                if ui
                    .add_enabled(track.is_some(), sync_btn)
                    .on_hover_text("Tempo an das andere Deck angleichen")
                    .clicked()
                {
                    app.sync_deck(idx);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    bpm_dial(ui, eff_bpm, pitch, color);
                });
            });
        });
    });
}

/// Toggles a 4-beat loop at the current playhead for a deck.
fn toggle_loop(app: &mut MixtreeApp, idx: usize) {
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
        let len = (4.0 * 60_000.0 / bpm) as u64; // 4 beats
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
    // Cue markers (suggested + manual).
    if dur_ms > 0 {
        for cue in cues {
            let cx = rect.left()
                + (cue.position_ms as f32 / dur_ms as f32).clamp(0.0, 1.0) * rect.width();
            let col = cue.color.as_deref().map(theme::hex).unwrap_or(theme::GREEN);
            painter.line_segment(
                [egui::pos2(cx, rect.top()), egui::pos2(cx, rect.bottom())],
                Stroke::new(1.5, col),
            );
            if let Some(label) = &cue.label {
                painter.text(
                    egui::pos2(cx + 2.0, rect.top() + 2.0),
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::FontId::proportional(8.0),
                    col,
                );
            }
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
    painter.text(
        rect.right_top() + Vec2::new(-6.0, 4.0),
        egui::Align2::RIGHT_TOP,
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
            ui.add_space(8.0);
            let mut a = app.eq_a;
            let mut b = app.eq_b;
            let mut xf = app.crossfader;
            let peak = app.audio.master_peak();
            let (ca, cb) = ui
                .horizontal(|ui| {
                    let ca = eq_column(ui, "A", &mut a);
                    ui.add_space(6.0);
                    vu_meter(ui, peak);
                    ui.add_space(6.0);
                    let cb = eq_column(ui, "B", &mut b);
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
            let auto = egui::Button::new(egui::RichText::new("Auto-Mix A→B").size(10.0))
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

/// EQ column. `eq` is `[low, mid, high]`; the sliders are shown HIGH→LOW to match
/// a physical mixer. Returns `true` if any band changed.
fn eq_column(ui: &mut egui::Ui, label: &str, eq: &mut [f32; 3]) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(label).size(10.0).color(theme::TEXT_DIM));
        for (name, i) in [("HIGH", 2usize), ("MID", 1), ("LOW", 0)] {
            changed |= ui
                .add(
                    egui::Slider::new(&mut eq[i], 0.0..=1.0)
                        .vertical()
                        .show_value(false)
                        .handle_shape(egui::style::HandleShape::Circle),
                )
                .changed();
            ui.label(egui::RichText::new(name).size(8.0).color(theme::TEXT_DIM));
        }
    });
    changed
}

/// Master VU meter: a vertical bar whose fill tracks the engine's output peak,
/// green → orange → red toward clipping.
fn vu_meter(ui: &mut egui::Ui, peak: f32) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new("VU").size(10.0).color(theme::TEXT_DIM));
        let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 96.0), Sense::hover());
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

fn mixflow(ui: &mut egui::Ui, app: &mut MixtreeApp) {
    ui.allocate_ui(Vec2::new(236.0, ui.available_height()), |ui| {
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
