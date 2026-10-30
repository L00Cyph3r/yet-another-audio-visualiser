mod analyzer;
pub mod api;
mod capture;
pub mod gui;
mod utils;
pub mod visualisations;

use crate::api::start_api_server;
use crate::gui::app::GuiApp;
use crate::gui::settings::{APP_DIR_NAME, SettingsStore};
use eframe::egui;
use log::info;
use std::net::SocketAddr;
use sysinfo::{ProcessRefreshKind, RefreshKind};

fn maybe_free_from_console() {
    #[cfg(target_os = "windows")]
    {
        use sysinfo::System;

        let sys = System::new_with_specifics(
            RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
        )
        .unwrap();

        if let Some(process) = sys.process(sysinfo::get_current_pid().unwrap()) {
            if let Some(parent_pid) = process.parent() {
                if let Some(parent_process) = sys.process(parent_pid) {
                    if parent_process.name().to_ascii_lowercase() == "explorer.exe" {
                        unsafe {
                            windows_sys::Win32::System::Console::FreeConsole();
                        }
                    }
                }
            }
        }
    }
}

fn main() -> eframe::Result {
    env_logger::init();
    let mut sentry_options = sentry::ClientOptions::default();
    sentry_options.release = sentry::release_name!();
    maybe_free_from_console();
    let _sentry_guard = sentry::init((
        "https://99b74881be00737427c1eed3f5104cf1@o4507225498845184.ingest.de.sentry.io/4512334762147920",
        sentry_options,
    ));

    let (command_tx, command_rx) = crossbeam_channel::unbounded();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 500.0])
            .with_min_inner_size([200.0, 50.0]),
        persist_window: true,
        ..Default::default()
    };

    eframe::run_native(
        APP_DIR_NAME,
        options,
        Box::new(move |cc| {
            let addr: SocketAddr = "127.0.0.1:3000".parse().expect("Invalid address");
            start_api_server(addr, command_tx, cc.egui_ctx.clone());

            let store = SettingsStore::default_location();
            let settings = match &store {
                Some(store) => {
                    info!("Using settings file {}", store.path().display());
                    store.load()
                }
                None => {
                    log::warn!("No configuration directory found; settings will not be saved");
                    Default::default()
                }
            };
            let mut gui_app = GuiApp::with_settings(command_rx, store, settings);
            // Fall back to the system default if no saved device could be restored.
            if !gui_app.has_device() {
                gui_app.set_device(utils::get_default_device());
            }
            Ok(Box::new(gui_app))
        }),
    )
}
