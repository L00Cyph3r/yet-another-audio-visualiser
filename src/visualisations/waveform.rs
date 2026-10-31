use crate::profiling::profile_function;
use crate::visualisations::{MAX_BANDS, Visualisation, VisualisationSettings, VisualisationType};
use eframe::emath::Vec2;
use egui::ViewportCommand::InnerSize;
use egui::{Color32, Ui};
use serde::{Deserialize, Serialize};

const MAX_WINDOW_SIZE: f32 = 8192.0;
/// Individual settings of the [`Waveform`] visualisation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Copy)]
#[serde(default)]
pub struct WaveformSettings {
    pub band_gap: f32,
    pub corner_radius: u8,
    pub color: Color32,
    pub window_width: f32,
    pub window_height: f32,
    pub bands: usize,
}

impl WaveformSettings {}

impl Default for WaveformSettings {
    fn default() -> Self {
        Self {
            band_gap: 2.0,
            corner_radius: 2,
            color: Color32::from_rgb(255, 10, 20),
            window_width: 800.0,
            window_height: 300.0,
            bands: 32,
        }
    }
}

impl WaveformSettings {
    /// Draws the settings editor. Returns `true` if any value was changed.
    pub fn settings_ui(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        egui::Grid::new("waveform_settings")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Window width");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.window_width)
                            .range(0.0..=MAX_WINDOW_SIZE)
                            .speed(1.0)
                            .suffix(" px"),
                    )
                    .on_hover_text("0 fits the width of the window")
                    .changed();
                ui.end_row();

                ui.label("Window height");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.window_height)
                            .range(0.0..=MAX_WINDOW_SIZE)
                            .speed(1.0)
                            .suffix(" px"),
                    )
                    .on_hover_text("0 fits the height of the window")
                    .changed();
                ui.end_row();

                ui.label("Bands");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.bands)
                            .range(1..=MAX_BANDS)
                            .speed(1),
                    )
                    .changed();
                ui.end_row();

                ui.label("Band gap");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.band_gap)
                            .range(0.0..=64.0)
                            .speed(1),
                    )
                    .changed();
                ui.end_row();

                ui.label("Corner radius");
                changed |= ui
                    .add(egui::DragValue::new(&mut self.corner_radius).range(0..=32))
                    .changed();
                ui.end_row();

                ui.label("Colour");
                changed |= ui.color_edit_button_srgba(&mut self.color).changed();
                ui.end_row();
            });
        changed
    }

    pub(crate) fn sanitize(&mut self) {
        self.window_width = self.window_width.clamp(0.0, MAX_WINDOW_SIZE);
        self.window_height = self.window_height.clamp(0.0, MAX_WINDOW_SIZE);
    }
}

pub(crate) struct Waveform(pub WaveformSettings);

impl Visualisation for Waveform {
    fn draw(&self, ui: &mut Ui, bands: &[f32], settings: &VisualisationSettings) {
        profile_function!();
        let own = &self.0;
        let window_size = Vec2::new(own.window_width, own.window_height);
        ui.ctx().send_viewport_cmd(InnerSize(window_size));
        let (rect, _) = ui.allocate_exact_size(window_size, egui::Sense::hover());
        let painter = ui.painter_at(rect);

        if bands.is_empty() {
            return;
        }

        let own = &settings.visualiser_settings.waveform;
        let gap = 0.0;
        let total_gap = gap * bands.len().saturating_sub(1) as f32;
        let bar_width = ((rect.width() - total_gap) / bands.len() as f32).max(1.0);

        for (i, &level) in bands.iter().enumerate() {
            let x = rect.left() + i as f32 * (bar_width + gap);
            let height = level * rect.height();

            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(x, rect.bottom() - height),
                egui::vec2(bar_width, height),
            );

            painter.rect_filled(
                bar_rect,
                egui::CornerRadius::same(own.corner_radius),
                own.color,
            );
        }
    }

    fn default_settings() -> VisualisationSettings {
        VisualisationSettings {
            visualiser: VisualisationType::Waveform(WaveformSettings::default()),
            ..VisualisationSettings::default()
        }
    }
}
