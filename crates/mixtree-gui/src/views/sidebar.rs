//! Left sidebar: genre tree, search, BPM filter and mix-goal selector.

use crate::app::MixtreeApp;
use crate::theme;
use mixtree_core::configuration::MixGoal;

const GOALS: &[(MixGoal, &str)] = &[
    (MixGoal::Neutral, "Neutral"),
    (MixGoal::RaiseEnergy, "Energie steigern"),
    (MixGoal::LowerEnergy, "Energie reduzieren"),
    (MixGoal::KeepGenre, "Genre halten"),
    (MixGoal::SwitchGenre, "Genre wechseln"),
    (MixGoal::HarmonicPriority, "Harmonisch"),
    (MixGoal::SurprisingButPlausible, "Überraschend"),
];

pub fn show(app: &mut MixtreeApp, ui: &mut egui::Ui) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("THEMEN / GENRES")
                .size(12.0)
                .color(theme::TEXT_DIM),
        );
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.search)
                .hint_text("Suche…")
                .desired_width(200.0),
        );
        if resp.changed() {
            app.mark_dirty();
        }
    });
    ui.add_space(8.0);

    // Genre tree (top-level with collapsible sub-genres).
    let tops: Vec<_> = app
        .genres
        .iter()
        .filter(|g| g.parent.is_none())
        .cloned()
        .collect();
    let children: Vec<_> = app
        .genres
        .iter()
        .filter(|g| g.parent.is_some())
        .cloned()
        .collect();

    egui::ScrollArea::vertical()
        .max_height(ui.available_height() - 150.0)
        .show(ui, |ui| {
            for g in &tops {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    let kids: Vec<_> = children
                        .iter()
                        .filter(|c| c.parent.as_deref() == Some(&g.id))
                        .collect();
                    let collapsed = app.collapsed_genres.contains(&g.id);
                    if !kids.is_empty() {
                        let tri = if collapsed { "▸" } else { "▾" };
                        if ui.add(egui::Button::new(tri).frame(false)).clicked() {
                            if collapsed {
                                app.collapsed_genres.remove(&g.id);
                            } else {
                                app.collapsed_genres.insert(g.id.clone());
                            }
                        }
                    } else {
                        ui.add_space(16.0);
                    }
                    genre_chip(ui, app, &g.id, &g.name, &g.color, true);
                });
                if !app.collapsed_genres.contains(&g.id) {
                    for c in children
                        .iter()
                        .filter(|c| c.parent.as_deref() == Some(&g.id))
                    {
                        ui.horizontal(|ui| {
                            ui.add_space(34.0);
                            genre_chip(ui, app, &c.id, &c.name, &c.color, false);
                        });
                    }
                }
            }
        });

    ui.add_space(8.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(format!("BPM {:.0}–{:.0}", app.bpm_range.0, app.bpm_range.1))
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
            let mut lo = app.bpm_range.0;
            let mut hi = app.bpm_range.1;
            let r1 = ui.add(egui::Slider::new(&mut lo, 60.0..=220.0).show_value(false));
            let r2 = ui.add(egui::Slider::new(&mut hi, 60.0..=220.0).show_value(false));
            if r1.changed() || r2.changed() {
                app.bpm_range = (lo.min(hi), hi.max(lo));
                app.mark_dirty();
            }

            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Mix-Ziel (roter Pfad)")
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
            let current = GOALS
                .iter()
                .find(|(g, _)| *g == app.goal)
                .map(|(_, l)| *l)
                .unwrap_or("Neutral");
            egui::ComboBox::from_id_salt("goal")
                .selected_text(current)
                .width(200.0)
                .show_ui(ui, |ui| {
                    for (goal, label) in GOALS {
                        if ui.selectable_label(app.goal == *goal, *label).clicked() {
                            app.goal = goal.clone();
                            app.mark_dirty();
                        }
                    }
                });

            if !app.selected_genres.is_empty()
                && ui
                    .button(format!(
                        "Filter zurücksetzen ({})",
                        app.selected_genres.len()
                    ))
                    .clicked()
            {
                app.selected_genres.clear();
                app.mark_dirty();
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            let import =
                egui::Button::new(egui::RichText::new("＋ Musikordner importieren").size(12.0))
                    .min_size(egui::vec2(206.0, 30.0));
            if ui.add(import).clicked() {
                app.import_folder_dialog();
            }
            let queued = app.queued_jobs();
            if queued > 0 {
                let label = if app.analyzing {
                    "Analysiere…".to_string()
                } else {
                    format!("Analysieren ({queued})")
                };
                let btn =
                    egui::Button::new(egui::RichText::new(label).size(12.0).color(theme::GREEN))
                        .min_size(egui::vec2(206.0, 28.0));
                if ui.add_enabled(!app.analyzing, btn).clicked() {
                    app.start_analysis();
                }
            }
        });
    });
}

fn genre_chip(
    ui: &mut egui::Ui,
    app: &mut MixtreeApp,
    id: &str,
    name: &str,
    color: &str,
    top: bool,
) {
    let selected = app.selected_genres.iter().any(|g| g == id);
    let dot = theme::hex(color);
    let frame = egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .rounding(egui::Rounding::same(6.0))
        .fill(if selected {
            egui::Color32::from_rgba_unmultiplied(dot.r(), dot.g(), dot.b(), 40)
        } else {
            egui::Color32::TRANSPARENT
        })
        .stroke(egui::Stroke::new(
            1.0,
            if selected {
                dot
            } else {
                egui::Color32::TRANSPARENT
            },
        ));
    let resp = frame
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 5.0, dot);
                let txt = egui::RichText::new(name)
                    .size(if top { 13.0 } else { 12.0 })
                    .color(if top { theme::TEXT } else { theme::TEXT_DIM });
                ui.label(if top { txt.strong() } else { txt });
            });
        })
        .response;
    if resp.interact(egui::Sense::click()).clicked() {
        app.toggle_genre(id);
    }
}
