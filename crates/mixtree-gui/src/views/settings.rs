//! Settings: scoring weights (configurable, spec §4) and library info.

use crate::app::MixtreeApp;
use crate::theme;

pub fn show(app: &mut MixtreeApp, ui: &mut egui::Ui) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(egui::RichText::new("Einstellungen").size(18.0).strong());
    });
    ui.add_space(10.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_max_width(520.0);
                ui.label(
                    egui::RichText::new("GEWICHTUNG DER ÜBERGANGSBEWERTUNG")
                        .size(12.0)
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(4.0);

                let w = &mut app.config.weights;
                let mut changed = false;
                changed |= slider(ui, "BPM-Kompatibilität", &mut w.bpm);
                changed |= slider(ui, "Harmonik (Key)", &mut w.key);
                changed |= slider(ui, "Energie", &mut w.energy);
                changed |= slider(ui, "Genre/Stil", &mut w.genre);
                changed |= slider(ui, "Mixability", &mut w.mixability);
                changed |= slider(ui, "Ziel-Ausrichtung", &mut w.goal);
                changed |= slider(ui, "Wiederholungs-Strafe", &mut w.repetition_penalty);

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Speichern").clicked() {
                        let cfg = app.config.clone();
                        if app.db.set_config(&cfg).is_ok() {
                            app.status = "Konfiguration gespeichert".into();
                        }
                        app.mark_dirty();
                    }
                    if ui.button("Zurücksetzen").clicked() {
                        app.config = mixtree_core::configuration::Configuration::default();
                        app.mark_dirty();
                    }
                });
                if changed {
                    app.mark_dirty();
                }

                ui.add_space(16.0);
                ui.separator();
                ui.label(
                    egui::RichText::new("DARSTELLUNG")
                        .size(12.0)
                        .color(theme::TEXT_DIM),
                );
                ui.checkbox(
                    &mut app.reduced_motion,
                    "Reduzierte Bewegung (Plasma/Beat-Ring statisch)",
                );

                ui.add_space(16.0);
                ui.separator();
                ui.label(
                    egui::RichText::new("INFO")
                        .size(12.0)
                        .color(theme::TEXT_DIM),
                );
                ui.label(
                    egui::RichText::new(if app.live {
                        "Persistente Bibliothek aktiv (SQLite im App-Datenverzeichnis)."
                    } else {
                        "In-Memory-Bibliothek (Fallback)."
                    })
                    .size(12.0)
                    .color(theme::TEXT_DIM),
                );
                ui.label(
                    egui::RichText::new(
                        "Native Rust-GUI (egui/eframe) · Kernlogik in mixtree-core.",
                    )
                    .size(12.0)
                    .color(theme::TEXT_DIM),
                );
            });
        });
    });
}

fn slider(ui: &mut egui::Ui, label: &str, value: &mut f64) -> bool {
    ui.horizontal(|ui| {
        ui.add_sized(
            [180.0, 18.0],
            egui::Label::new(egui::RichText::new(label).size(12.0)),
        );
        ui.add(egui::Slider::new(value, 0.0..=2.0)).changed()
    })
    .inner
}
