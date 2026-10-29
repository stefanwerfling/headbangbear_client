//! Right sidebar: next-best-track recommendation, alternatives, red path, tips.

use crate::app::MixtreeApp;
use crate::theme;
use crate::views::cap;

pub fn show(app: &mut MixtreeApp, ui: &mut egui::Ui) {
    ui.add_space(12.0);
    // Section header inside a red-outlined rounded panel with a transition glyph,
    // as in the reference.
    egui::Frame::none()
        .fill(egui::Color32::from_rgba_unmultiplied(0xff, 0x2d, 0x55, 16))
        .stroke(egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_unmultiplied(0xff, 0x2d, 0x55, 130),
        ))
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .outer_margin(egui::Margin::symmetric(10.0, 0.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) =
                    ui.allocate_exact_size(egui::vec2(18.0, 14.0), egui::Sense::hover());
                transition_icon(ui.painter(), r, theme::RED_SOFT);
                ui.label(
                    egui::RichText::new("Empfohlener nächster Schritt")
                        .size(14.0)
                        .strong()
                        .color(theme::RED_SOFT),
                );
            });
        });
    ui.add_space(8.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(2.0);
        let recs = app.recs.clone();
        let current_title = app
            .current
            .and_then(|id| app.track(id))
            .map(|t| t.title)
            .unwrap_or_default();

        let Some(best) = recs.first() else {
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("Keine Empfehlung — Bibliothek leer oder stark gefiltert.")
                        .color(theme::TEXT_DIM),
                );
            });
            return;
        };

        // Recommended card.
        egui::Frame::none()
            .fill(egui::Color32::from_rgba_unmultiplied(0xff, 0x2d, 0x55, 18))
            .stroke(egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(0xff, 0x2d, 0x55, 110),
            ))
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::same(13.0))
            .outer_margin(egui::Margin::symmetric(10.0, 0.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    thumb(ui, app, best.track_id, 46.0, theme::RED, false);
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(&best.title).size(16.0).strong());
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · BPM {} · Energie {}%",
                                cap(best.genre.as_deref()),
                                best.bpm
                                    .map(|b| format!("{b:.0}"))
                                    .unwrap_or_else(|| "—".into()),
                                best.energy
                                    .map(|e| e.to_string())
                                    .unwrap_or_else(|| "—".into()),
                            ))
                            .size(11.0)
                            .color(theme::TEXT_DIM),
                        );
                    });
                });
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!("Übergang von: {current_title}"))
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if let Some(d) = best.bpm_diff {
                        badge(ui, format!("{d:+.0} BPM"), theme::RED, egui::Color32::WHITE);
                    }
                    // "Perfekter Übergang" for a strong score, else the percentage.
                    let q = (best.score.total * 100.0).round();
                    let txt = if q >= 85.0 {
                        "Perfekter Übergang".to_string()
                    } else {
                        format!("Übergangsqualität {q}%")
                    };
                    ui.label(egui::RichText::new(txt).size(11.0).color(theme::RED_SOFT));
                });
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(&best.reason)
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(8.0);
                let btn = egui::Button::new(
                    egui::RichText::new("▶  Als nächsten Track laden")
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(theme::RED)
                .min_size(egui::vec2(ui.available_width(), 32.0));
                if ui.add(btn).clicked() {
                    app.load_next(best.track_id);
                }
            });

        ui.add_space(14.0);
        section(ui, "ALTERNATIVEN");
        for r in recs.iter().skip(1) {
            let resp = egui::Frame::none()
                .fill(theme::PANEL2)
                .stroke(egui::Stroke::new(1.0, theme::LINE))
                .rounding(egui::Rounding::same(9.0))
                .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                .outer_margin(egui::Margin::symmetric(10.0, 3.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        thumb(ui, app, r.track_id, 30.0, theme::CYAN, true);
                        ui.add_space(2.0);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(&r.title).size(13.0).strong());
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} · BPM {} · {} · E{}%",
                                    cap(r.genre.as_deref()),
                                    r.bpm
                                        .map(|b| format!("{b:.0}"))
                                        .unwrap_or_else(|| "—".into()),
                                    r.key.clone().unwrap_or_else(|| "—".into()),
                                    r.energy
                                        .map(|e| e.to_string())
                                        .unwrap_or_else(|| "—".into()),
                                ))
                                .size(10.0)
                                .color(theme::TEXT_DIM),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(d) = r.bpm_diff {
                                let col = if d <= 0.0 { theme::CYAN } else { theme::GREEN };
                                badge(
                                    ui,
                                    format!("{d:+.0} BPM"),
                                    egui::Color32::from_rgba_unmultiplied(
                                        col.r(),
                                        col.g(),
                                        col.b(),
                                        40,
                                    ),
                                    col,
                                );
                            }
                        });
                    });
                })
                .response;
            if resp.interact(egui::Sense::click()).clicked() {
                app.load_next(r.track_id);
            }
        }

        // Red path summary.
        if let Some(path) = app.path.clone() {
            if path.track_ids.len() > 1 {
                ui.add_space(12.0);
                section(ui, "ROTER PFAD");
                let names: Vec<String> = path
                    .track_ids
                    .iter()
                    .map(|id| {
                        app.track(*id)
                            .map(|t| t.title)
                            .unwrap_or_else(|| id.to_string())
                    })
                    .collect();
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(names.join("  »  "))
                            .size(11.0)
                            .color(theme::RED_SOFT),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "Ø Übergangsqualität {}%",
                            (path.average_score * 100.0).round()
                        ))
                        .size(10.0)
                        .color(theme::TEXT_DIM),
                    );
                });
            }
        }

        ui.add_space(12.0);
        section(ui, "ÜBERGANGS-TIPPS");
        tip(ui, "bpm", "BPM", "Halte den Unterschied unter ±8 BPM.");
        tip(ui, "energy", "Energie", "Steigere die Energie schrittweise.");
        tip(
            ui,
            "harmonic",
            "Harmonik",
            "Achte auf ähnliche Stimmung & Tonart.",
        );
        tip(
            ui,
            "loop",
            "CUE / LOOP",
            "Cues für präzise Übergänge (Audio folgt).",
        );
        ui.add_space(8.0);
    });
}

/// A small rounded cover thumbnail for a track, reusing the tree's decoded
/// cover texture when available (else a dim placeholder). With `play`, overlays
/// a small play triangle, matching the reference's alternative rows.
fn thumb(
    ui: &mut egui::Ui,
    app: &MixtreeApp,
    id: i64,
    size: f32,
    border: egui::Color32,
    play: bool,
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let p = ui.painter();
    let rounding = egui::Rounding::same(7.0);
    match app.node_textures.get(&id) {
        Some(tex) => {
            p.image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        None => {
            p.rect_filled(rect, rounding, theme::PANEL2);
        }
    }
    p.rect_stroke(rect, rounding, egui::Stroke::new(1.0, border));
    if play {
        let c = rect.center();
        let s = size * 0.22;
        p.circle_filled(c, s + 3.0, egui::Color32::from_black_alpha(130));
        p.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(c.x - s * 0.5, c.y - s),
                egui::pos2(c.x - s * 0.5, c.y + s),
                egui::pos2(c.x + s, c.y),
            ],
            egui::Color32::WHITE,
            egui::Stroke::NONE,
        ));
    }
}

/// A small "send / transition" paper-plane glyph pointing right.
fn transition_icon(p: &egui::Painter, rect: egui::Rect, col: egui::Color32) {
    let l = rect.left();
    let r = rect.right();
    let cy = rect.center().y;
    let top = rect.top();
    let bot = rect.bottom();
    // Upper wing + lower body as two triangles forming a paper plane.
    p.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(l, top),
            egui::pos2(r, cy),
            egui::pos2(l, cy),
        ],
        col,
        egui::Stroke::NONE,
    ));
    p.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(l, cy),
            egui::pos2(r, cy),
            egui::pos2(l, bot),
        ],
        egui::Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 150),
        egui::Stroke::NONE,
    ));
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(title).size(11.0).color(theme::TEXT_DIM));
    });
    ui.add_space(4.0);
}

fn badge(ui: &mut egui::Ui, text: String, bg: egui::Color32, fg: egui::Color32) {
    egui::Frame::none()
        .fill(bg)
        .rounding(egui::Rounding::same(5.0))
        .inner_margin(egui::Margin::symmetric(7.0, 2.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(11.0).strong().color(fg));
        });
}

fn tip(ui: &mut egui::Ui, kind: &str, title: &str, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let (r, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
        tip_icon(ui.painter(), r, kind, theme::CYAN);
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(title)
                    .size(12.0)
                    .strong()
                    .color(theme::CYAN),
            );
            ui.label(egui::RichText::new(text).size(10.0).color(theme::TEXT_DIM));
        });
    });
    ui.add_space(4.0);
}

/// A small drawn glyph for a transition tip (the font lacks these icons).
fn tip_icon(p: &egui::Painter, rect: egui::Rect, kind: &str, col: egui::Color32) {
    let c = rect.center();
    let s = rect.width() * 0.4;
    let w = egui::Stroke::new(1.6, col);
    match kind {
        // Gauge: a semicircle arc with a needle.
        "bpm" => {
            let n = 10;
            let pts: Vec<egui::Pos2> = (0..=n)
                .map(|i| {
                    let a = std::f32::consts::PI + i as f32 / n as f32 * std::f32::consts::PI;
                    egui::pos2(c.x + a.cos() * s, c.y + s * 0.3 + a.sin() * s)
                })
                .collect();
            for seg in pts.windows(2) {
                p.line_segment([seg[0], seg[1]], w);
            }
            p.line_segment(
                [egui::pos2(c.x, c.y + s * 0.3), egui::pos2(c.x + s * 0.5, c.y - s * 0.4)],
                w,
            );
        }
        // Lightning bolt as a zigzag polyline (a bolt is non-convex, so a filled
        // convex polygon would render wrong).
        "energy" => {
            let pts = [
                egui::pos2(c.x + s * 0.35, c.y - s),
                egui::pos2(c.x - s * 0.35, c.y - s * 0.1),
                egui::pos2(c.x + s * 0.2, c.y - s * 0.1),
                egui::pos2(c.x - s * 0.35, c.y + s),
            ];
            for seg in pts.windows(2) {
                p.line_segment([seg[0], seg[1]], egui::Stroke::new(2.0, col));
            }
        }
        // Harmonic: two sine-ish humps.
        "harmonic" => {
            let n = 16;
            let pts: Vec<egui::Pos2> = (0..=n)
                .map(|i| {
                    let t = i as f32 / n as f32;
                    let x = c.x - s + t * 2.0 * s;
                    let y = c.y - (t * std::f32::consts::TAU).sin() * s * 0.6;
                    egui::pos2(x, y)
                })
                .collect();
            for seg in pts.windows(2) {
                p.line_segment([seg[0], seg[1]], w);
            }
        }
        // Loop: a ring with an arrowhead.
        _ => {
            p.circle_stroke(c, s * 0.8, w);
            let tip = egui::pos2(c.x + s * 0.8, c.y - s * 0.3);
            p.add(egui::Shape::convex_polygon(
                vec![
                    tip,
                    egui::pos2(tip.x - s * 0.5, tip.y - s * 0.1),
                    egui::pos2(tip.x - s * 0.1, tip.y + s * 0.45),
                ],
                col,
                egui::Stroke::NONE,
            ));
        }
    }
}
