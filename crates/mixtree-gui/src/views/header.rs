//! Top header bar: brand, navigation, live status, current metrics, clock.

use crate::app::{MixtreeApp, View};
use crate::theme;
use crate::views::key_str;

pub fn show(app: &mut MixtreeApp, ui: &mut egui::Ui) {
    ui.horizontal_centered(|ui| {
        ui.add_space(14.0);
        // Brand mark (drawn, so no font-glyph dependency).
        let (logo, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
        let p = ui.painter();
        p.circle_filled(logo.center(), 11.0, theme::RED);
        p.circle_filled(logo.center(), 5.0, theme::BG2);
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("MixTree").size(18.0).strong());
            ui.label(
                egui::RichText::new("Music. Connected.")
                    .size(10.0)
                    .color(theme::TEXT_DIM),
            );
        });

        ui.add_space(24.0);
        // Navigation tabs.
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            nav(ui, app, View::Tree, "Tree");
            nav(ui, app, View::Library, "Library");
            nav(ui, app, View::Settings, "Settings");
        });

        // Right side status.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(14.0);
            // Clock.
            ui.label(egui::RichText::new(clock()).color(theme::TEXT));
            ui.add_space(8.0);
            // Goal pill.
            pill(ui, goal_label(app), theme::RED_SOFT);
            // Current track metrics.
            if let Some(cur) = app.current.and_then(|id| app.track(id)) {
                ui.add_space(6.0);
                let metrics = format!(
                    "{}  ·  {}  ·  E{}",
                    cur.bpm
                        .map(|b| format!("{b:.0} BPM"))
                        .unwrap_or_else(|| "— BPM".into()),
                    key_str(cur.key),
                    cur.energy
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "—".into()),
                );
                ui.label(egui::RichText::new(metrics).color(theme::TEXT));
            }
            ui.add_space(8.0);
            // Live / demo pill.
            let (txt, col) = if app.live {
                ("Live Mix", theme::GREEN)
            } else {
                ("Demo", theme::ORANGE)
            };
            pill(ui, txt.to_string(), col);
            if !app.status.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(&app.status)
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                );
            }
        });
    });
}

fn nav(ui: &mut egui::Ui, app: &mut MixtreeApp, view: View, label: &str) {
    let active = app.view == view;
    let text = egui::RichText::new(label)
        .color(if active { theme::TEXT } else { theme::TEXT_DIM })
        .size(13.0);
    let btn = egui::Button::new(text)
        .fill(if active {
            theme::PANEL2
        } else {
            egui::Color32::TRANSPARENT
        })
        .stroke(egui::Stroke::new(
            1.0,
            if active {
                theme::CYAN
            } else {
                egui::Color32::TRANSPARENT
            },
        ));
    if ui.add(btn).clicked() {
        app.view = view;
    }
}

fn pill(ui: &mut egui::Ui, text: String, color: egui::Color32) {
    egui::Frame::none()
        .fill(theme::PANEL2)
        .stroke(egui::Stroke::new(1.0, theme::LINE2))
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::symmetric(10.0, 4.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(11.0).color(color));
        });
}

fn goal_label(app: &MixtreeApp) -> String {
    use mixtree_core::configuration::MixGoal::*;
    match app.goal {
        Neutral => "Neutral",
        RaiseEnergy => "Energie ↑",
        LowerEnergy => "Energie ↓",
        KeepGenre => "Genre halten",
        SwitchGenre => "Genre wechseln",
        HarmonicPriority => "Harmonisch",
        SurprisingButPlausible => "Überraschend",
        TargetTrack(_) => "Zieltrack",
        TargetGenre(_) => "Zielgenre",
    }
    .to_string()
}

/// Local wall-clock `HH:MM`. (egui has no time source; use std.)
fn clock() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mins = (secs / 60) % 60;
    let hours = (secs / 3600) % 24;
    format!("{hours:02}:{mins:02}")
}
