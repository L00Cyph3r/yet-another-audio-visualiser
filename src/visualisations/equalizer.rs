use crate::visualisations::{Visualisation, VisualisationSettings, VisualisationType};
use egui::{Color32, Ui};

pub(crate) struct Equalizer;

impl Visualisation for Equalizer {
    fn draw(&self, ui: &mut Ui, bands: &[f32], settings: &VisualisationSettings) {
        let (rect, _) = ui.allocate_exact_size(settings.window_size, egui::Sense::hover());
        let painter = ui.painter_at(rect);

        if bands.is_empty() {
            return;
        }

        let total_gap = settings.band_gap * bands.len().saturating_sub(1) as f32;
        let bar_width = ((rect.width() - total_gap) / bands.len() as f32).max(1.0);

        for (i, &level) in bands.iter().enumerate() {
            let x = rect.left() + i as f32 * (bar_width + settings.band_gap);
            let height = level * rect.height();
            let color = volume_to_color(level);

            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(x, rect.bottom() - height),
                egui::vec2(bar_width, height),
            );

            painter.rect_filled(
                bar_rect,
                egui::CornerRadius::same(2),
                color,
                // ui.visuals().widgets.active.bg_fill,
            );
        }
    }

    fn default_settings() -> VisualisationSettings {
        VisualisationSettings {
            visualiser: VisualisationType::Equalizer,
            bands: 64,
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
