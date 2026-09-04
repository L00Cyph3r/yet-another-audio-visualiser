use crate::analyzer::SpectrumAnalyzer;
use crate::capture::{AudioCapture, start_capture};
use crate::utils::{device_name_format, get_audio_devices};
use crate::visualisations::{Visualisation, VisualisationSettings, VisualisationType};
use cpal::traits::DeviceTrait;
use eframe::Storage;
use egui::{IdSalt, InnerResponse, Label, WidgetText};
use strum::IntoEnumIterator;

pub struct GuiApp {
    available_devices: Vec<cpal::Device>,
    device: Option<cpal::Device>,
    capture: Option<AudioCapture>,
    analyzer: Option<SpectrumAnalyzer>,
    error: Option<String>,
    visualisation: VisualisationSettings,
    show_help: bool,
}

impl GuiApp {
    pub fn new() -> Self {
        let mut available_devices = get_audio_devices();
        available_devices.sort_by(|a, b| {
            a.description()
                .unwrap()
                .name()
                .cmp(&b.description().unwrap().name())
        });
        Self {
            available_devices,
            device: None,
            capture: None,
            analyzer: None,
            error: None,
            visualisation: VisualisationSettings::default(),
            show_help: false,
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
                        devices.iter().enumerate().for_each(|(i, device)| {
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
                let label_id = IdSalt::new(label.text());
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
    // fn save(&mut self, storage: &mut dyn Storage) {
    //     // storage.
    // }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
                let capture = start_capture(device).expect("Failed to start audio capture");
                let analyzer = SpectrumAnalyzer::new(capture.sample_rate);

                self.capture = Some(capture);
                self.analyzer = Some(analyzer);
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
