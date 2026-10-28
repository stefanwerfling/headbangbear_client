//! Right sidebar: next-best-track recommendation, alternatives, red path, tips.

use crate::app::MixtreeApp;
use crate::theme;
use crate::views::cap;

pub fn show(app: &mut MixtreeApp, ui: &mut egui::Ui) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("Empfohlener nächster Schritt")
                .size(14.0)
                .strong()
                .color(theme::RED_SOFT),
        );
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
                    ui.label(
                        egui::RichText::new(format!(
                            "Übergangsqualität {}%",
                            (best.score.total * 100.0).round()
                        ))
                        .size(11.0)
                        .color(theme::RED_SOFT),
                    );
                });
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(&best.reason)
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(8.0);
                let btn = egui::Button::new(
                    egui::RichText::new("Als nächsten Track laden")
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
                        ui.label(egui::RichText::new("›").size(14.0).color(theme::CYAN));
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
        tip(ui, "BPM", "Halte den Unterschied unter ±8 BPM.");
        tip(ui, "Energie", "Steigere die Energie schrittweise.");
        tip(ui, "Harmonik", "Achte auf ähnliche Stimmung & Tonart.");
        tip(
            ui,
            "CUE / LOOP",
            "Cues für präzise Übergänge (Audio folgt).",
        );
        ui.add_space(8.0);
    });
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

fn tip(ui: &mut egui::Ui, title: &str, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(10.0);
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
