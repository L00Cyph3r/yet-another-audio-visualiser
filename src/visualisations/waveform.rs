use crate::visualisations::{Visualisation, VisualisationSettings, VisualisationType};
use egui::{Color32, Ui};

pub(crate) struct Waveform;

impl Visualisation for Waveform {
    fn draw(&self, ui: &mut Ui, bands: &[f32], settings: &VisualisationSettings) {
        let (rect, _) = ui.allocate_exact_size(settings.window_size, egui::Sense::hover());

        let painter = ui.painter_at(rect);

        if bands.is_empty() {
            return;
        }

        let gap = 0.0;
        let total_gap = gap * bands.len().saturating_sub(1) as f32;
        let bar_width = ((rect.width() - total_gap) / bands.len() as f32).max(1.0);

        for (i, &level) in bands.iter().enumerate() {
            let x = rect.left() + i as f32 * (bar_width + gap);
            let height = level * rect.height();
            let color = Color32::from_rgb(255, 10, 20);

            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(x, rect.bottom() - height),
                egui::vec2(bar_width, height),
            );

            painter.rect_filled(bar_rect, egui::CornerRadius::same(2), color);
        }
    }

    fn default_settings() -> VisualisationSettings {
        VisualisationSettings {
            visualiser: VisualisationType::Waveform,
            bands: 64,
            ..VisualisationSettings::default()
        }
    }
}
