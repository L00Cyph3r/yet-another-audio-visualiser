#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod analyzer;
mod capture;
pub mod gui;
mod utils;
pub mod visualisations;

use crate::gui::app::GuiApp;
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 500.0])
            .with_min_inner_size([200.0, 50.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Photostyle's Audio Widget",
        options,
        Box::new(|_cc| Ok(Box::new(GuiApp::new()))),
    )
}
