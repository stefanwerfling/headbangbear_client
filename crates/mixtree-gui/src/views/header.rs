//! Top header bar: brand, navigation, live status, settings, clock.

use crate::app::{MixtreeApp, View};
use crate::theme;
use egui::{Color32, Pos2, Sense, Stroke, Vec2};

pub fn show(app: &mut MixtreeApp, ui: &mut egui::Ui) {
    ui.horizontal_centered(|ui| {
        ui.add_space(14.0);
        // Brand mark: a small neon equalizer/waveform glyph (drawn, so no
        // font-glyph dependency), matching the reference logo.
        let (logo, _) = ui.allocate_exact_size(egui::vec2(28.0, 26.0), Sense::hover());
        waveform_logo(ui.painter(), logo, theme::RED);
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
            nav(ui, app, View::Playlist, "Playlist");
            nav(ui, app, View::Library, "Library");
            nav(ui, app, View::Settings, "Settings");
        });

        // Right side status.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(14.0);
            // Settings gear (far right, as in the reference).
            let (gear, gresp) =
                ui.allocate_exact_size(egui::vec2(24.0, 24.0), Sense::click());
            let gear_col = if app.view == View::Settings || gresp.hovered() {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            };
            gear_icon(ui.painter(), gear, gear_col);
            if gresp.on_hover_text("Einstellungen").clicked() {
                app.view = View::Settings;
            }
            ui.add_space(10.0);
            // Clock.
            ui.label(egui::RichText::new(clock()).color(theme::TEXT));
            ui.add_space(10.0);
            // Live / demo pill (with a leading status dot).
            let (txt, col) = if app.live {
                ("Live Mix", theme::GREEN)
            } else {
                ("Demo", theme::ORANGE)
            };
            live_pill(ui, txt, col);
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

/// A small equalizer brand mark: four rounded neon bars of varying height.
fn waveform_logo(p: &egui::Painter, rect: egui::Rect, col: Color32) {
    let heights = [0.45f32, 0.9, 0.65, 1.0];
    let n = heights.len();
    let gap = 3.0;
    let bw = (rect.width() - gap * (n as f32 - 1.0)) / n as f32;
    for (i, h) in heights.iter().enumerate() {
        let x = rect.left() + i as f32 * (bw + gap);
        let bh = rect.height() * h;
        let bar = egui::Rect::from_min_size(
            Pos2::new(x, rect.center().y - bh / 2.0),
            Vec2::new(bw, bh),
        );
        // Soft glow + bright core.
        p.rect_filled(
            bar.expand(1.5),
            egui::Rounding::same(2.0),
            Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 50),
        );
        p.rect_filled(bar, egui::Rounding::same(2.0), col);
    }
}

/// A simple gear glyph: a ring with short radial teeth and a hub.
fn gear_icon(p: &egui::Painter, rect: egui::Rect, col: Color32) {
    let c = rect.center();
    let r = rect.width() * 0.32;
    for k in 0..8 {
        let a = k as f32 / 8.0 * std::f32::consts::TAU;
        let dir = Vec2::angled(a);
        p.line_segment(
            [c + dir * r, c + dir * (r + 3.0)],
            Stroke::new(2.0, col),
        );
    }
    p.circle_stroke(c, r, Stroke::new(2.0, col));
    p.circle_filled(c, r * 0.35, col);
}

/// The live/demo pill with a leading coloured status dot.
fn live_pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::none()
        .fill(theme::PANEL2)
        .stroke(Stroke::new(1.0, theme::LINE2))
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::symmetric(10.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, color);
                ui.label(egui::RichText::new(text).size(11.0).color(color));
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
