//! Library table with inline BPM/Key correction and re-analysis (spec §7, §8).

use crate::app::MixtreeApp;
use crate::theme;
use crate::views::{cap, key_str};
use mixtree_core::analysis::AnalysisKind;
use mixtree_core::camelot::CamelotKey;
use mixtree_core::database::schema::ANALYSIS_VERSION;

pub fn show(app: &mut MixtreeApp, ui: &mut egui::Ui) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(egui::RichText::new("Bibliothek").size(18.0).strong());
        ui.label(
            egui::RichText::new(format!("{} Tracks", app.tracks.len()))
                .size(12.0)
                .color(theme::TEXT_DIM),
        );
    });
    ui.add_space(8.0);

    let tracks = app.tracks.clone();
    let genre_name: std::collections::HashMap<String, String> = app
        .genres
        .iter()
        .map(|g| (g.id.clone(), g.name.clone()))
        .collect();

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("library_grid")
                .striped(true)
                .spacing(egui::vec2(16.0, 6.0))
                .min_col_width(60.0)
                .show(ui, |ui| {
                    for h in [
                        "Titel",
                        "Künstler",
                        "Genre",
                        "BPM",
                        "Key",
                        "Energie",
                        "Status",
                        "Decks",
                        "Analyse",
                    ] {
                        ui.label(
                            egui::RichText::new(h)
                                .size(12.0)
                                .strong()
                                .color(theme::TEXT_DIM),
                        );
                    }
                    ui.end_row();

                    for t in &tracks {
                        let editing = app.editing == Some(t.id);
                        ui.label(egui::RichText::new(&t.title).color(if t.missing {
                            theme::TEXT_DIM
                        } else {
                            theme::TEXT
                        }));
                        ui.label(t.artist.clone().unwrap_or_else(|| "—".into()));
                        ui.label(
                            genre_name
                                .get(t.subgenre.as_deref().or(t.genre.as_deref()).unwrap_or(""))
                                .cloned()
                                .unwrap_or_else(|| cap(t.genre.as_deref())),
                        );

                        // BPM (editable).
                        if editing {
                            ui.add(
                                egui::TextEdit::singleline(&mut app.edit_bpm).desired_width(52.0),
                            );
                        } else {
                            ui.label(
                                t.bpm
                                    .map(|b| format!("{b:.0}"))
                                    .unwrap_or_else(|| "—".into()),
                            );
                        }
                        // Key (editable).
                        if editing {
                            ui.add(
                                egui::TextEdit::singleline(&mut app.edit_key)
                                    .desired_width(44.0)
                                    .hint_text("8A"),
                            );
                        } else {
                            ui.label(key_str(t.key));
                        }

                        // Energy bar.
                        match t.energy {
                            Some(e) => {
                                ui.horizontal(|ui| {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(50.0, 5.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(
                                        rect,
                                        egui::Rounding::same(2.0),
                                        theme::LINE,
                                    );
                                    let w = rect.width() * (e as f32 / 100.0);
                                    ui.painter().rect_filled(
                                        egui::Rect::from_min_size(
                                            rect.min,
                                            egui::vec2(w, rect.height()),
                                        ),
                                        egui::Rounding::same(2.0),
                                        theme::GREEN,
                                    );
                                    ui.label(egui::RichText::new(format!("{e}%")).size(11.0));
                                });
                            }
                            None => {
                                ui.label("—");
                            }
                        }

                        // Analysis state.
                        let state = format!("{:?}", t.analysis_state).to_lowercase();
                        let col = match t.analysis_state {
                            mixtree_core::models::AnalysisState::Done => theme::GREEN,
                            mixtree_core::models::AnalysisState::Failed => theme::RED,
                            _ => theme::TEXT_DIM,
                        };
                        ui.label(egui::RichText::new(state).size(10.0).color(col));

                        // Deck load buttons.
                        ui.horizontal(|ui| {
                            if ui.small_button("A").clicked() {
                                app.set_current(t.id);
                            }
                            if ui.small_button("B").clicked() {
                                app.load_next(t.id);
                            }
                        });

                        // Edit / reanalyze.
                        ui.horizontal(|ui| {
                            if editing {
                                if ui.small_button("OK").clicked() {
                                    save_correction(app, t.id);
                                }
                                if ui.small_button("X").clicked() {
                                    app.editing = None;
                                }
                            } else {
                                if ui
                                    .small_button("Edit")
                                    .on_hover_text("BPM/Key korrigieren")
                                    .clicked()
                                {
                                    app.editing = Some(t.id);
                                    app.edit_bpm =
                                        t.bpm.map(|b| format!("{b:.0}")).unwrap_or_default();
                                    app.edit_key = t.key.map(|k| k.to_string()).unwrap_or_default();
                                }
                                if ui
                                    .add_enabled(app.live, egui::Button::new("Re").small())
                                    .on_hover_text("Neu analysieren")
                                    .clicked()
                                {
                                    reanalyze(app, t.id);
                                }
                            }
                        });
                        ui.end_row();
                    }
                });

            if tracks.is_empty() {
                ui.add_space(20.0);
                ui.label(
                    egui::RichText::new(
                        "Keine Tracks. Importiere einen Musikordner oder passe die Filter an.",
                    )
                    .color(theme::TEXT_DIM),
                );
            }
        });
}

fn save_correction(app: &mut MixtreeApp, id: i64) {
    if let Ok(Some(mut t)) = app.db.get_track(id) {
        if app.edit_bpm.trim().is_empty() {
            // leave bpm unchanged on empty
        } else if let Ok(b) = app.edit_bpm.trim().parse::<f64>() {
            t.bpm = Some(b);
        }
        if app.edit_key.trim().is_empty() {
            t.key = None;
        } else if let Ok(k) = app.edit_key.trim().parse::<CamelotKey>() {
            t.key = Some(k);
        }
        if app.db.update_track(&t).is_ok() {
            app.status = format!("{} korrigiert", t.title);
        }
    }
    app.editing = None;
    app.mark_dirty();
}

fn reanalyze(app: &mut MixtreeApp, id: i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    for kind in [
        AnalysisKind::Bpm,
        AnalysisKind::Key,
        AnalysisKind::Energy,
        AnalysisKind::Structure,
    ] {
        let _ = app.db.enqueue_job(id, kind, ANALYSIS_VERSION, now);
    }
    app.status = "Analyse eingereiht".into();
}
