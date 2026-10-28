use crate::visualisations::{MAX_BANDS, Visualisation, VisualisationSettings, VisualisationType};
use eframe::emath::Vec2;
use egui::ViewportCommand::InnerSize;
use egui::{Color32, Ui};
use serde::{Deserialize, Serialize};

const MAX_WINDOW_SIZE: f32 = 8192.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqualizerSettings {
    pub bands: usize,
    pub band_gap: f32,
    pub corner_radius: u8,
    pub color_by_volume: bool,
    pub color: Color32,
    pub window_width: f32,
    pub window_height: f32,
}

impl EqualizerSettings {
    pub(crate) fn sanitize(&mut self) {
        self.window_width = self.window_width.clamp(0.0, MAX_WINDOW_SIZE);
        self.window_height = self.window_height.clamp(0.0, MAX_WINDOW_SIZE);
    }
}

impl Default for EqualizerSettings {
    fn default() -> Self {
        Self {
            band_gap: 2.0,
            corner_radius: 2,
            color_by_volume: true,
            color: Color32::from_rgb(0, 200, 255),
            window_width: 800.0,
            window_height: 300.0,
            bands: 32,
        }
    }
}

impl EqualizerSettings {
    /// Draws the settings editor. Returns `true` if any value was changed.
    pub fn settings_ui(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        egui::Grid::new("equalizer_settings")
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

                ui.label("Band gap");
                ui.add(egui::Slider::new(&mut self.band_gap, 0.0..=64.0));
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

                ui.label("Corner radius");
                changed |= ui
                    .add(egui::DragValue::new(&mut self.corner_radius).range(0..=32))
                    .changed();
                ui.end_row();

                ui.label("Colour by volume");
                changed |= ui.checkbox(&mut self.color_by_volume, "").changed();
                ui.end_row();

                ui.add_enabled_ui(!self.color_by_volume, |ui| {
                    ui.label("Colour");
                });
                ui.add_enabled_ui(!self.color_by_volume, |ui| {
                    changed |= ui.color_edit_button_srgba(&mut self.color).changed();
                });
                ui.end_row();
            });
        changed
    }
}

pub(crate) struct Equalizer;

impl Visualisation for Equalizer {
    fn draw(&self, ui: &mut Ui, bands: &[f32], settings: &VisualisationSettings) {
        {
            let own = &settings.visualiser_settings.equalizer;
            ui.ctx()
                .send_viewport_cmd(InnerSize(Vec2::new(own.window_width, own.window_height)));
        }
        let (rect, _) = ui.allocate_exact_size(settings.window_size, egui::Sense::hover());
        let painter = ui.painter_at(rect);

        if bands.is_empty() {
            return;
        }

        let own = &settings.visualiser_settings.equalizer;
        let total_gap = settings.band_gap * bands.len().saturating_sub(1) as f32;
        let bar_width = ((rect.width() - total_gap) / bands.len() as f32).max(1.0);

        for (i, &level) in bands.iter().enumerate() {
            let x = rect.left() + i as f32 * (bar_width + settings.band_gap);
            let height = level * rect.height() * settings.gain;
            let color = if own.color_by_volume {
                volume_to_color(level * settings.gain)
            } else {
                own.color
            };

            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(x, rect.bottom() - height),
                egui::vec2(bar_width, height),
            );

            painter.rect_filled(bar_rect, egui::CornerRadius::same(own.corner_radius), color);
        }
    }

    fn default_settings() -> VisualisationSettings {
        VisualisationSettings {
            visualiser: VisualisationType::Equalizer,
            ..VisualisationSettings::default()
        }
    }
}

fn volume_to_color(volume: f32) -> Color32 {
    // Map volume (0.0..1.0) to hue (240° blue to 0° red)
    // Low volume = blue (240°), high volume = red (0°)
    let hue = (0.9 - volume.clamp(0.0, 0.9)) * 120.0;

    // Convert HSV to RGB (with saturation=1.0, value=1.0)
    let h = hue / 60.0;
    let i = h.floor() as i32;
    let f = h - i as f32;

    let (r, g, b) = match i % 6 {
        0 => (1.0, f, 0.0),
        1 => (1.0 - f, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, 1.0 - f, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, 1.0 - f),
    };

    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}
