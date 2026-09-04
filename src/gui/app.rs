use crate::analyzer::{SpectrumAnalyzer, SpectrumAnalyzerSettings};
use crate::api::types::{AppCommand, DeviceDto, SettingsDto};
use crate::capture::{AudioCapture, start_capture};
use crate::utils::{device_name_format, get_audio_devices};
use crate::visualisations::{Visualisation, VisualisationSettings, VisualisationType};
use cpal::traits::DeviceTrait;
use crossbeam_channel::Receiver;
use egui::load::DefaultTextureLoader;
use egui::{IdSalt, InnerResponse, Label};
use strum::IntoEnumIterator;

pub struct GuiApp {
    available_devices: Vec<cpal::Device>,
    device: Option<cpal::Device>,
    capture: Option<AudioCapture>,
    analyzer: Option<SpectrumAnalyzer>,
    error: Option<String>,
    visualisation: VisualisationSettings,
    show_help: bool,
    command_rx: Receiver<AppCommand>,
}

impl GuiApp {
    pub fn new(command_rx: Receiver<AppCommand>) -> Self {
        let mut available_devices = get_audio_devices();
        available_devices.sort_by(|a, b| {
            let desc_a = a.description().ok();
            let desc_b = b.description().ok();
            let name_a = desc_a.as_ref().map(|d| d.name()).unwrap_or_default();
            let name_b = desc_b.as_ref().map(|d| d.name()).unwrap_or_default();
            name_a.cmp(name_b)
        });
        Self {
            available_devices,
            device: None,
            capture: None,
            analyzer: None,
            error: None,
            visualisation: VisualisationSettings::default(),
            show_help: false,
            command_rx,
        }
    }

    pub fn get_settings_dto(&self) -> SettingsDto {
        let active_device = self
            .device
            .as_ref()
            .and_then(|d| d.description().ok().map(|desc| desc.name().to_string()));
        SettingsDto {
            visualiser: self.visualisation.visualiser,
            bands: self.visualisation.bands,
            gain: self.visualisation.gain,
            smoothing: self.visualisation.smoothing,
            band_gap: self.visualisation.band_gap,
            active_device,
        }
    }

    pub fn handle_commands(&mut self) {
        while let Ok(cmd) = self.command_rx.try_recv() {
            match cmd {
                AppCommand::UpdateSettings(patch, respond_to) => {
                    if let Some(visualiser) = patch.visualiser {
                        self.visualisation.visualiser = visualiser;
                    }
                    if let Some(bands) = patch.bands {
                        self.visualisation.bands = bands;
                    }
                    if let Some(gain) = patch.gain {
                        self.visualisation.gain = gain;
                    }
                    if let Some(smoothing) = patch.smoothing {
                        self.visualisation.smoothing = smoothing;
                    }
                    if let Some(band_gap) = patch.band_gap {
                        self.visualisation.band_gap = band_gap;
                    }
                    if let Some(tx) = respond_to {
                        let _ = tx.send(self.get_settings_dto());
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
                        self.device = Some(dev);
                        self.capture = None;
                        self.analyzer = None;
                        self.error = None;
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
        if self.device.is_none() {
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
                                ui.selectable_value(
                                    &mut self.device,
                                    Some(device.clone()),
                                    device_name_format(Some(device)),
                                );
                            }
                        })
                    })
            });
        } else {
            ui.horizontal_wrapped(|ui| {
                let label = Label::new("Audio Input").selectable(false);
                ui.add(label);
                ui.label(device_name_format(self.device.as_ref()));
            });
        }
    }

    fn draw_visualisation_section(
        &mut self,
        ui: &mut egui::Ui,
    ) -> InnerResponse<InnerResponse<Option<()>>> {
        ui.horizontal_wrapped(|ui| {
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
        })
    }

    fn draw_help(&mut self, ui: &mut egui::Ui) {
        ui.label("Press Escape to exit.");
    }
}

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_commands();

        ui.ctx().input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                self.device = None;
                self.capture = None;
                self.analyzer = None;
            }
            if i.key_pressed(egui::Key::H) {
                self.show_help = !self.show_help;
            }
        });

        ui.set_pixels_per_point(1.0);
        ui.ctx().request_repaint();

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(4.0);
            if self.show_help {
                self.draw_help(ui);
            }
            self.draw_device_selection(ui);
            self.draw_visualisation_section(ui);
            if self.device.is_some() && self.capture.is_none() && self.analyzer.is_none() {
                let device = self.device.clone().unwrap();
                match start_capture(device) {
                    Ok(capture) => {
                        let analyzer = SpectrumAnalyzer::new(SpectrumAnalyzerSettings {
                            sample_rate: capture.sample_rate,
                            ..SpectrumAnalyzerSettings::default()
                        });
                        self.capture = Some(capture);
                        self.analyzer = Some(analyzer);
                        self.error = None;
                    }
                    Err(err) => {
                        self.error = Some(err.to_string());
                    }
                }
            }

            if let (Some(capture), Some(analyzer)) = (&self.capture, &mut self.analyzer) {
                while let Ok(sample) = capture.samples.try_recv() {
                    analyzer.push_sample(sample);
                }

                analyzer.update();
            }

            if let Some(capture) = &self.capture {
                ui.label(format!(
                    "Input: {}  •  {} Hz",
                    capture.device_name, capture.sample_rate
                ));
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
                self.visualisation.window_size = ui.available_size();
                self.visualisation
                    .visualiser
                    .draw(ui, analyzer.bands(), &self.visualisation);
            }
            ui.add_space(8.0);
        });
    }
}
