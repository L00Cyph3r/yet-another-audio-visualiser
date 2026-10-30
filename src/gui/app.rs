use crate::analyzer::{SpectrumAnalyzer, SpectrumAnalyzerSettings};
use crate::api::types::{AppCommand, DeviceDto, SettingsDto, SettingsPatch};
use crate::capture::{AudioCapture, start_capture};
use crate::gui::settings::{AppSettings, SettingsStore};
use crate::utils::{device_name_format, get_audio_devices};
use crate::visualisations::{Visualisation, VisualisationSettings, VisualisationType};
use cpal::Device;
use cpal::traits::DeviceTrait;
use crossbeam_channel::Receiver;
use egui::{IdSalt, InnerResponse, Label};
use egui::ViewportCommand::StartDrag;
use egui::{Id, IdSalt, InnerResponse, Label, Sense};
use log::{debug, error, info, warn};
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use strum::IntoEnumIterator;

/// How long the settings have to stay unchanged before they are written to disk, so that
/// dragging a slider does not write the configuration file on every frame.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub struct GuiApp {
    available_devices: Vec<Device>,
    device: Option<Device>,
    capture: Option<AudioCapture>,
    analyzer: Option<SpectrumAnalyzer>,
    error: Option<String>,
    visualisation: VisualisationSettings,
    show_help: bool,
    command_rx: Receiver<AppCommand>,
    restart_analysis: bool,
    restart_capture: bool,
    frame_history: FrameHistory,
    /// Where the settings are persisted. `None` disables persistence.
    store: Option<SettingsStore>,
    /// Name of the input device the user selected last; this is what gets persisted.
    /// It is kept separately from `device`, which is also cleared by pressing Escape.
    persisted_device: Option<String>,
    /// The settings as they were when the previous frame ended, to detect changes made by
    /// the GUI or the API.
    last_seen: VisualisationSettings,
    /// Set when there are unsaved changes; the time of the most recent change.
    dirty_since: Option<Instant>,
}

fn device_name(device: &Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_string())
}

/// Makes the whole visualisation window a drag handle, so that pressing the left mouse button
/// anywhere in it moves the native window — the same gesture a title bar offers.
///
/// The visualisation window has no other interactive widgets, so its entire surface can be used
/// for this. The area is registered with [`Ui::interact`] rather than allocated, because the
/// visualisation itself allocates the available space for drawing.
///
/// [`Sense::DRAG`] is used instead of [`Sense::click_and_drag`] on purpose: a drag-only widget
/// starts the drag on the same frame the button goes down (no movement threshold to wait for),
/// and it is not focusable, so it cannot steal keyboard focus from the main window.
///
/// Returns `true` if a window drag was started this frame.
fn start_window_drag_on_press(ui: &mut egui::Ui) -> bool {
    let response = ui.interact(
        ui.max_rect(),
        Id::unique("visualisation-window-drag"),
        Sense::DRAG,
    );
    // Only the left button moves the window, matching what the platform expects of `StartDrag`.
    let started = response.drag_started_by(egui::PointerButton::Primary);
    if started {
        ui.ctx().send_viewport_cmd(StartDrag);
    }
    started
}

impl GuiApp {
    pub(crate) fn set_device(&mut self, device: Option<Device>) {
        self.restart_capture = device.is_some();
        self.device = device;
    }

    pub(crate) fn has_device(&self) -> bool {
        self.device.is_some()
    }

    fn mark_dirty(&mut self) {
        self.dirty_since = Some(Instant::now());
    }

    fn save_settings(&mut self) {
        self.dirty_since = None;
        let Some(store) = &self.store else {
            return;
        };
        let settings = AppSettings {
            device: self.persisted_device.clone(),
            visualisation: self.visualisation.clone(),
        };
        match store.save(&settings) {
            Ok(()) => debug!("Saved settings to {}", store.path().display()),
            Err(e) => error!("Failed to save settings to {}: {e}", store.path().display()),
        }
    }

    fn persist_settings_if_needed(&mut self) {
        if self.visualisation != self.last_seen {
            self.last_seen = self.visualisation.clone();
            self.mark_dirty();
        }
        if self
            .dirty_since
            .is_some_and(|changed| changed.elapsed() >= SAVE_DEBOUNCE)
        {
            self.save_settings();
        }
    }
}

#[derive(Debug)]
pub struct FrameHistory {
    frame_times: VecDeque<f32>,
    max_history: usize,
}

impl Default for FrameHistory {
    fn default() -> Self {
        Self {
            frame_times: VecDeque::with_capacity(100),
            max_history: 100,
        }
    }
}

impl FrameHistory {
    pub fn add_frametime(&mut self, dt: f32) {
        if self.frame_times.len() >= self.max_history {
            self.frame_times.pop_front();
        }
        self.frame_times.push_back(dt);
    }

    pub fn average_fps(&self) -> f32 {
        if self.frame_times.is_empty() {
            return 0.0;
        }
        let total_time: f32 = self.frame_times.iter().sum();
        let mean_time = total_time / self.frame_times.len() as f32;

        if mean_time > 0.0 {
            1.0 / mean_time
        } else {
            0.0
        }
    }
}

impl GuiApp {
    pub fn new(command_rx: Receiver<AppCommand>) -> Self {
        Self::with_settings(command_rx, None, AppSettings::default())
    }

    pub fn with_settings(
        command_rx: Receiver<AppCommand>,
        store: Option<SettingsStore>,
        settings: AppSettings,
    ) -> Self {
        let mut available_devices = get_audio_devices();
        available_devices.sort_by(|a, b| {
            let desc_a = a.description().ok();
            let desc_b = b.description().ok();
            let name_a = desc_a.as_ref().map(|d| d.name()).unwrap_or_default();
            let name_b = desc_b.as_ref().map(|d| d.name()).unwrap_or_default();
            name_a.cmp(name_b)
        });
        debug!("Available devices: {:?}", available_devices);
        info!("Initialising GuiApp");

        let mut visualisation = settings.visualisation;
        visualisation.sanitize();
        // The stored `visualiser_settings` are what survive a restart, so populate the active
        // visualisation's tuple from them before rendering.
        visualisation.load_visualiser_settings();
        visualisation.sanitize();

        let restored_device = settings.device.as_ref().and_then(|wanted| {
            let found = available_devices.iter().find(|d| {
                d.description().is_ok_and(|desc| {
                    desc.direction() == cpal::DeviceDirection::Input && desc.name() == wanted
                })
            });
            if found.is_none() {
                warn!("Saved audio input device '{wanted}' is not available");
            }
            found.cloned()
        });

        Self {
            available_devices,
            restart_capture: restored_device.is_some(),
            device: restored_device,
            capture: None,
            analyzer: None,
            error: None,
            last_seen: visualisation.clone(),
            visualisation,
            show_help: false,
            command_rx,
            restart_analysis: true,
            frame_history: FrameHistory::default(),
            store,
            persisted_device: settings.device,
            dirty_since: None,
        }
    }

    pub fn get_settings_dto(&self) -> SettingsDto {
        let active_device = self.device.as_ref().and_then(device_name);
        SettingsDto {
            visualiser: self.visualisation.visualiser.clone(),
            gain: self.visualisation.gain,
            smoothing: self.visualisation.smoothing,
            visualiser_settings: self.visualisation.visualiser_settings.clone(),
            active_device,
        }
    }

    /// Applies an API patch. The patch is applied completely or, if any part of it is invalid,
    /// not at all. Values are clamped into their valid range.
    pub fn apply_patch(&mut self, patch: &SettingsPatch) -> Result<(), String> {
        let mut updated = self.visualisation.clone();
        if let Some(visualiser) = patch.visualiser.clone() {
            updated.visualiser = visualiser;
            // Switching visualisation should pick up the persisted settings for that type,
            // matching what happens when the user switches it in the GUI.
            updated.load_visualiser_settings();
        }
        if let Some(gain) = patch.gain {
            updated.gain = gain;
        }
        if let Some(smoothing) = patch.smoothing {
            updated.smoothing = smoothing;
        }
        if let Some(band_gap) = patch.band_gap {
            updated.band_gap = band_gap;
        }
        if let Some(visualiser_patch) = &patch.visualiser_settings {
            updated.visualiser_settings.apply_patch(visualiser_patch)?;
            // If the patch touched the currently active visualisation, reflect it in its tuple.
            updated.load_visualiser_settings();
        }
        updated.sanitize();

        // Keep the persisted copy in sync with the active visualisation's tuple.
        updated.sync_visualiser_settings();

        // Only the number of bands affects the analyser; everything else is applied while drawing.
        self.visualisation = updated;
        Ok(())
    }

    pub fn handle_commands(&mut self) {
        while let Ok(cmd) = self.command_rx.try_recv() {
            match cmd {
                AppCommand::UpdateSettings(patch, respond_to) => {
                    let result = self.apply_patch(&patch).map(|()| self.get_settings_dto());
                    match respond_to {
                        Some(tx) => {
                            let _ = tx.send(result);
                        }
                        None => {
                            if let Err(err) = result {
                                warn!("Rejected settings update: {err}");
                            }
                        }
                    }
                }
                AppCommand::SelectDevice(name, respond_to) => {
                    let found = self
                        .available_devices
                        .iter()
                        .find(|d| {
                            if let Ok(desc) = d.description() {
                                desc.name() == name || device_name_format(Some(d)) == name
                            } else {
                                false
                            }
                        })
                        .cloned();

                    let result = if let Some(dev) = found {
                        self.persisted_device = device_name(&dev);
                        self.mark_dirty();
                        self.device = Some(dev);
                        self.capture = None;
                        self.analyzer = None;
                        self.error = None;
                        self.restart_capture = true;
                        self.restart_analysis = true;
                        Ok(())
                    } else {
                        Err(format!("Device '{}' not found", name))
                    };

                    if let Some(tx) = respond_to {
                        let _ = tx.send(result);
                    }
                }
                AppCommand::GetSettings(tx) => {
                    let _ = tx.send(self.get_settings_dto());
                }
                AppCommand::GetDevices(tx) => {
                    let devices = self
                        .available_devices
                        .iter()
                        .filter_map(|d| {
                            let desc = d.description().ok()?;
                            if desc.direction() != cpal::DeviceDirection::Input {
                                return None;
                            }
                            Some(DeviceDto {
                                name: desc.name().to_string(),
                                formatted_name: device_name_format(Some(d)),
                                supports_input: desc.supports_input(),
                                supports_output: desc.supports_output(),
                                device_type: desc.device_type().to_string(),
                                direction: desc.direction().to_string(),
                                address: desc.address().unwrap_or("Unknown").to_string(),
                                device_id: format!("{}", d.id().unwrap()),
                                driver: desc.driver().unwrap_or("Unknown").to_string(),
                                manufacturer: desc.manufacturer().unwrap_or("Unknown").to_string(),
                            })
                        })
                        .collect();
                    let _ = tx.send(devices);
                }
            }
        }
    }

    fn draw_device_selection(&mut self, ui: &mut egui::Ui) {
        let devices = &self.available_devices;
        ui.horizontal_wrapped(|ui| {
            let label = Label::new("Audio Input").selectable(false);
            let label_id = IdSalt::new(label.text());
            ui.add(label);

            egui::ComboBox::from_id_salt(label_id)
                .selected_text(device_name_format(self.device.as_ref()))
                .show_ui(ui, |ui| {
                    devices.iter().for_each(|device| {
                        if let Ok(description) = device.description() {
                            if description.direction() != cpal::DeviceDirection::Input {
                                return;
                            }
                            if ui
                                .selectable_value(
                                    &mut self.device,
                                    Some(device.clone()),
                                    device_name_format(Some(device)),
                                )
                                .clicked()
                            {
                                info!("Device changed to {}", device_name_format(Some(device)));
                                self.restart_capture = true;
                                self.restart_analysis = true;
                                self.persisted_device = device_name(device);
                                self.dirty_since = Some(Instant::now());
                            }
                        }
                    })
                })
        });
        return;
    }

    fn draw_visualisation_section(
        &mut self,
        ui: &mut egui::Ui,
    ) -> InnerResponse<InnerResponse<Option<()>>> {
        let before = self.visualisation.visualiser.clone();
        let response = ui.horizontal_wrapped(|ui| {
            let label = Label::new("Visualisation").selectable(false);
            let label_id = IdSalt::new(label.text());
            ui.add(label);

            egui::ComboBox::from_id_salt(label_id)
                .selected_text(self.visualisation.visualiser.to_string())
                .show_ui(ui, |ui| {
                    for visualisation_type in VisualisationType::iter() {
                        ui.selectable_value(
                            &mut self.visualisation.visualiser,
                            visualisation_type.clone(),
                            visualisation_type.to_string(),
                        );
                    }
                })
        });
        // When the user switches to another visualisation, instantiate it from the stored
        // settings for that type rather than from a fresh default.
        if self.visualisation.visualiser != before {
            self.visualisation.load_visualiser_settings();
        }
        response
    }

    /// Draws the editors for the shared settings and for the individual settings of the
    /// selected visualisation.
    fn draw_settings_section(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("General settings")
            .default_open(true)
            .show(ui, |ui| {
                let settings = &mut self.visualisation;
                egui::Grid::new("general_settings")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Gain");
                        ui.add(egui::Slider::new(&mut settings.gain, 0.0..=10.0));
                        ui.end_row();

                        ui.label("Smoothing");
                        ui.add(egui::Slider::new(&mut settings.smoothing, 0.0..=1.0));
                        ui.end_row();
                    });
            });

        let visualiser = &self.visualisation.visualiser;
        egui::CollapsingHeader::new(format!("{visualiser} settings"))
            .id_salt(("visualiser_settings", visualiser.to_string()))
            .default_open(true)
            .show(ui, |ui| {
                if self.visualisation.visualiser.settings_ui(ui) {
                    self.visualisation.visualiser.sanitize();
                    self.visualisation.visualiser_settings.sanitize();
                    // Persist the unchanged value: write the edited tuple into `visualiser_settings`
                    // so the change is saved to disk.
                    self.visualisation.sync_visualiser_settings();
                }
                ui.add_space(4.0);
                if ui.button("Reset to defaults").clicked() {
                    self.visualisation.visualiser_settings =
                        VisualisationSettings::default().visualiser_settings;
                    // Reset the active visualisation's tuple and keep the persisted copy in sync.
                    self.visualisation.load_visualiser_settings();
                    self.visualisation.sync_visualiser_settings();
                }
            });

        let bands_before = match self.analyzer.as_ref() {
            Some(analyzer) => analyzer.bands().len(),
            None => 0,
        };
        let should_restart = match &self.visualisation.visualiser {
            VisualisationType::Equalizer(s) => s.bands != bands_before,
            VisualisationType::Waveform(s) => s.bands != bands_before,
            VisualisationType::Nyan(s) => false,
            VisualisationType::SegmentedSpectrumBars(s) => s.bands != bands_before,
        };
        self.restart_analysis |= should_restart;
    }

    // TODO: Make proper help window
    fn draw_help(&mut self, ui: &mut egui::Ui) {
        ui.label("Press Escape to exit.");
    }
}

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_commands();
        let typing = ui.ctx().egui_wants_keyboard_input();
        ui.ctx().input(|i| {
            if typing {
                return;
            }
            if i.key_pressed(egui::Key::Escape) {
                self.device = None;
                self.capture = None;
                self.analyzer = None;
            }
            if i.key_pressed(egui::Key::F1) {
                self.show_help = !self.show_help;
            }
        });

        ui.ctx()
            .request_repaint_after(Duration::from_micros(1_000_000 / 60));
        ui.set_pixels_per_point(1.0);
        self.frame_history.add_frametime(ui.input(|i| i.stable_dt));

        egui::Panel::top("top_panel").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("FPS: {:.0}", self.frame_history.average_fps()));
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(4.0);
            if self.show_help {
                self.draw_help(ui);
            }
            self.draw_device_selection(ui);
            self.draw_visualisation_section(ui);
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .max_height(ui.available_height().max(150.0))
                .show(ui, |ui| self.draw_settings_section(ui));
            if self.analyzer.is_none() && self.capture.is_some() {
                self.analyzer = Some(SpectrumAnalyzer::new(SpectrumAnalyzerSettings {
                    sample_rate: self.capture.as_ref().unwrap().sample_rate,
                    fft_size: 16384,
                    bands: self.visualisation.bands(),
                    gain: 1.0,
                    ..SpectrumAnalyzerSettings::default()
                }));
                self.restart_analysis = true;
            }
            if self.restart_capture && self.device.is_some() {
                let device = self.device.clone().unwrap();
                match start_capture(device) {
                    Ok(capture) => {
                        self.restart_capture = false;
                        self.capture = Some(capture);
                    }
                    Err(err) => {
                        self.error = Some(err.to_string());
                    }
                }
            }
            if self.restart_analysis && self.capture.is_some() && self.device.is_some() {
                let analyzer = SpectrumAnalyzer::new(SpectrumAnalyzerSettings {
                    sample_rate: self.capture.as_ref().unwrap().sample_rate,
                    fft_size: 2048,
                    bands: self.visualisation.bands(),
                    gain: self.visualisation.gain,
                    ..SpectrumAnalyzerSettings::default()
                });
                self.analyzer = Some(analyzer);
                self.restart_analysis = false;
            }

            if let (Some(capture), Some(analyzer)) = (&self.capture, &mut self.analyzer) {
                while let Ok(sample) = capture.samples.try_recv() {
                    analyzer.push_sample(sample);
                }

                analyzer.update();
            }

            if let Some(error) = &self.error {
                ui.colored_label(
                    egui::Color32::RED,
                    format!("Could not start audio capture: {error}"),
                );

                ui.add_space(8.0);
                ui.label("Check that an input device is connected and available.");
                return;
            }

            if let Some(analyzer) = &self.analyzer {
                let analyzer = analyzer;
                ui.ctx().show_viewport_immediate(
                    egui::ViewportId::from_hash_of("visualisation"),
                    egui::ViewportBuilder::default()
                        .with_title("Visualisation")
                        .with_inner_size([1024.0, 512.0]),
                    |ui, class| {
                        if class == egui::ViewportClass::Immediate {
                            // The window has no interactive widgets, so the whole surface can be
                            // used to move it around.
                            start_window_drag_on_press(ui);
                            self.visualisation.window_size = ui.available_size();
                            self.visualisation.visualiser.draw(
                                ui,
                                analyzer.bands(),
                                &self.visualisation,
                            );
                        }
                    },
                );
            }
            ui.add_space(8.0);
        });

        self.persist_settings_if_needed();
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.persist_settings_if_needed();
        if self.dirty_since.is_some() {
            self.save_settings();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window the visualisation is drawn into, in points.
    fn screen_rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))
    }

    /// Runs a single egui pass over [`start_window_drag_on_press`] with the given input events and
    /// returns the viewport commands egui emitted for the window.
    fn run_pass(ctx: &egui::Context, events: Vec<egui::Event>) -> Vec<egui::ViewportCommand> {
        let input = egui::RawInput {
            screen_rect: Some(screen_rect()),
            events,
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            start_window_drag_on_press(ui);
        });

        let commands = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or_else(Vec::new, |viewport| viewport.commands.clone());
        // `FullOutput` debug-asserts when dropped with unapplied texture deltas.
        output.drop_without_applying_deltas();
        commands
    }

    fn button_event(pos: egui::Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Registers the drag area with one pass, so that egui can hit-test against it afterwards.
    fn register_drag_area(ctx: &egui::Context) {
        run_pass(ctx, Vec::new());
    }

    #[test]
    fn press_starts_a_window_drag() {
        let ctx = egui::Context::default();
        let pos = screen_rect().center();
        register_drag_area(&ctx);

        let commands = run_pass(
            &ctx,
            vec![
                egui::Event::PointerMoved(pos),
                button_event(pos, egui::PointerButton::Primary, true),
            ],
        );

        assert!(
            commands.contains(&StartDrag),
            "expected StartDrag, got {commands:?}"
        );
    }

    #[test]
    fn hover_without_press_does_not_start_a_window_drag() {
        let ctx = egui::Context::default();
        let pos = screen_rect().center();
        register_drag_area(&ctx);

        let commands = run_pass(&ctx, vec![egui::Event::PointerMoved(pos)]);

        assert!(!commands.contains(&StartDrag), "got {commands:?}");
    }

    #[test]
    fn non_primary_press_does_not_start_a_window_drag() {
        let ctx = egui::Context::default();
        let pos = screen_rect().center();
        register_drag_area(&ctx);

        let commands = run_pass(
            &ctx,
            vec![
                egui::Event::PointerMoved(pos),
                button_event(pos, egui::PointerButton::Secondary, true),
            ],
        );

        assert!(!commands.contains(&StartDrag), "got {commands:?}");
    }

    #[test]
    fn window_drag_starts_again_after_the_button_is_released() {
        let ctx = egui::Context::default();
        let start = screen_rect().center();
        let moved = start + egui::vec2(40.0, 25.0);
        register_drag_area(&ctx);

        // Press: the window starts moving.
        let pressed = run_pass(
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                button_event(start, egui::PointerButton::Primary, true),
            ],
        );
        assert!(pressed.contains(&StartDrag), "got {pressed:?}");

        // Moving while held must not ask to start dragging again.
        let dragging = run_pass(&ctx, vec![egui::Event::PointerMoved(moved)]);
        assert!(!dragging.contains(&StartDrag), "got {dragging:?}");

        // Release: the OS move loop has ended.
        let released = run_pass(
            &ctx,
            vec![
                egui::Event::PointerMoved(moved),
                button_event(moved, egui::PointerButton::Primary, false),
            ],
        );
        assert!(!released.contains(&StartDrag), "got {released:?}");

        // A second press on the same spot must move the window again.
        let pressed_again = run_pass(
            &ctx,
            vec![
                egui::Event::PointerMoved(moved),
                button_event(moved, egui::PointerButton::Primary, true),
            ],
        );
        assert!(pressed_again.contains(&StartDrag), "got {pressed_again:?}");
    }
}
