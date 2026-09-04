#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod analyzer;
pub mod api;
mod capture;
pub mod gui;
mod utils;
pub mod visualisations;

use std::net::SocketAddr;
use crate::api::start_api_server;
use crate::gui::app::GuiApp;
use eframe::egui;

fn main() -> eframe::Result {
    env_logger::init();

    let (command_tx, command_rx) = crossbeam_channel::unbounded();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 500.0])
            .with_min_inner_size([200.0, 50.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Photostyle's Audio Widget",
        options,
        Box::new(move |cc| {
            let addr: SocketAddr = "127.0.0.1:3000".parse().expect("Invalid address");
            start_api_server(addr, command_tx, cc.egui_ctx.clone());
            Ok(Box::new(GuiApp::new(command_rx)))
        }),
    )
}
