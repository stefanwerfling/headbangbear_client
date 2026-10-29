//! Thin bottom status bar (spec/reference footer): current theme focus on the
//! left, live BPM / key / energy of the current track on the right.

use crate::app::MixtreeApp;
use crate::theme;
use crate::views::{cap, key_str};

pub fn show(app: &mut MixtreeApp, ui: &mut egui::Ui) {
    ui.horizontal_centered(|ui| {
        ui.add_space(16.0);

        // Left: "Aktueller Theme-Fokus: <current genre> ▸ <next genre>".
        let current = app.current.and_then(|id| app.track(id));
        let from_genre = current.as_ref().and_then(|t| t.genre.clone());
        let to_genre = app.recs.first().and_then(|r| r.genre.clone());
        ui.label(
            egui::RichText::new("Aktueller Theme-Fokus:")
                .size(11.0)
                .color(theme::TEXT_DIM),
        );
        ui.label(
            egui::RichText::new(cap(from_genre.as_deref()))
                .size(11.0)
                .strong()
                .color(theme::TEXT),
        );
        if to_genre.is_some() {
            ui.label(egui::RichText::new("▶").size(9.0).color(theme::RED_SOFT));
            ui.label(
                egui::RichText::new(cap(to_genre.as_deref()))
                    .size(11.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.label(
                egui::RichText::new("(Empfohlener Flow)")
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
        }

        // Right: live metrics of the current track.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(16.0);
            let (bpm, key, energy) = match &current {
                Some(t) => (
                    t.bpm
                        .map(|b| format!("{b:.1}"))
                        .unwrap_or_else(|| "—".into()),
                    key_str(t.key),
                    t.energy
                        .map(|e| format!("{e}%"))
                        .unwrap_or_else(|| "—".into()),
                ),
                None => ("—".into(), "—".into(), "—".into()),
            };
            metric(ui, theme::RED, "Energy", &energy);
            metric(ui, theme::CYAN, "Key", &key);
            metric(ui, theme::GREEN, "BPM", &bpm);
        });
    });
}

/// One right-aligned metric chip: a small coloured dot, a dim label, a value.
fn metric(ui: &mut egui::Ui, dot: egui::Color32, label: &str, value: &str) {
    ui.add_space(14.0);
    ui.label(
        egui::RichText::new(value)
            .size(11.0)
            .strong()
            .color(theme::TEXT),
    );
    ui.label(egui::RichText::new(label).size(11.0).color(theme::TEXT_DIM));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, dot);
}
