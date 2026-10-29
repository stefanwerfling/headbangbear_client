//! Playlist / Mix-Queue view: the recommended red-path flow as an ordered set
//! of tracks, plus the already-played history. Click a row to make it current.

use crate::app::MixtreeApp;
use crate::theme;
use crate::views::{cap, key_str};
use egui::{Color32, Sense, Stroke, Vec2};

pub fn show(app: &mut MixtreeApp, ui: &mut egui::Ui) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Deine Playlist").size(18.0).strong());
            ui.label(
                egui::RichText::new("Der empfohlene Flow als Reihenfolge — Klick lädt den Track.")
                    .size(12.0)
                    .color(theme::TEXT_DIM),
            );
        });
    });
    ui.add_space(10.0);

    // Build the ordered queue from the recommended red path (falling back to the
    // current track + next-best recommendations when no path is available).
    let mut queue: Vec<i64> = Vec::new();
    if let Some(path) = &app.path {
        queue.extend(path.track_ids.iter().copied());
    }
    if queue.is_empty() {
        if let Some(t) = &app.deck_a.track {
            queue.push(t.id);
        }
        queue.extend(app.recs.iter().map(|r| r.track_id));
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(4.0);
        if queue.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new("Noch keine Playlist — Track im Baum wählen.")
                        .color(theme::TEXT_DIM),
                );
            });
            return;
        }
        let mut prev_bpm: Option<f64> = None;
        for (i, id) in queue.iter().enumerate() {
            let Some(t) = app.track(*id) else { continue };
            let active = app.current == Some(*id);
            let dot = dot_color(t.genre.as_deref());
            let diff = match (prev_bpm, t.bpm) {
                (Some(p), Some(b)) => Some(b - p),
                _ => None,
            };
            prev_bpm = t.bpm;

            let resp = egui::Frame::none()
                .fill(if active { theme::PANEL2 } else { theme::PANEL })
                .stroke(Stroke::new(1.0, if active { theme::RED } else { theme::LINE }))
                .rounding(egui::Rounding::same(8.0))
                .inner_margin(egui::Margin::symmetric(12.0, 9.0))
                .outer_margin(egui::Margin::symmetric(16.0, 3.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{:02}", i + 1))
                                .size(13.0)
                                .color(theme::TEXT_DIM),
                        );
                        let (r, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 5.0, dot);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(&t.title).size(14.0).strong());
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} · BPM {} · {} · Energie {}",
                                    cap(t.genre.as_deref()),
                                    t.bpm
                                        .map(|b| format!("{b:.0}"))
                                        .unwrap_or_else(|| "—".into()),
                                    key_str(t.key),
                                    t.energy
                                        .map(|e| format!("{e}%"))
                                        .unwrap_or_else(|| "—".into()),
                                ))
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(d) = diff {
                                let col = if d.abs() <= 8.0 { theme::GREEN } else { theme::ORANGE };
                                badge(ui, format!("{d:+.0} BPM"), col);
                            }
                        });
                    });
                })
                .response;
            if resp.interact(Sense::click()).clicked() {
                app.set_current(*id);
            }
        }
        ui.add_space(8.0);
    });
}

fn badge(ui: &mut egui::Ui, text: String, col: Color32) {
    egui::Frame::none()
        .fill(Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 40))
        .rounding(egui::Rounding::same(5.0))
        .inner_margin(egui::Margin::symmetric(7.0, 2.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(11.0).strong().color(col));
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
