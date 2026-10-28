//! MixTree native GUI entry point (eframe/egui, cross-platform: Windows/macOS/Linux).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod theme;
mod views;

use app::MixtreeApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([1100.0, 720.0])
            .with_title("MixTree — Music. Connected."),
        ..Default::default()
    };
    eframe::run_native(
        "MixTree",
        options,
        Box::new(|cc| Ok(Box::new(MixtreeApp::new(cc)))),
    )
}
