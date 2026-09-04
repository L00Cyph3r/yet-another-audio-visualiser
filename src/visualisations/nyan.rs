use std::collections::VecDeque;

use crate::visualisations::{Visualisation, VisualisationSettings, VisualisationType};
use egui::{Color32, Pos2, Stroke, Ui};

const RAINBOW_BANDS: usize = 6;

pub(crate) struct Nyan;

impl Visualisation for Nyan {
    fn draw(&self, ui: &mut Ui, bands: &[f32], settings: &VisualisationSettings) {
        let (rect, _) = ui.allocate_exact_size(settings.window_size, egui::Sense::hover());

        if bands.is_empty() || rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }

        // Nyan is recreated by the enum dispatcher each frame, so the history
        // lives in egui's temporary widget state instead of in `Nyan` itself.
        let history_id = ui.id().with("nyan-rainbow-history");
        let history_size = rect.width().ceil() as usize + 1;
        let history = ui.ctx().data_mut(|data| {
            let history =
                data.get_temp_mut_or_default::<VecDeque<[f32; RAINBOW_BANDS]>>(history_id);
            history.push_back(to_rainbow_bands(bands));
            while history.len() > history_size {
                history.pop_front();
            }
            history.clone()
        });

        let painter = ui.painter_at(rect);

        // Clementine's rainbow consists of six parallel, coloured traces.
        // Each trace is displaced by its latest spectrum value, while older
        // samples remain to the left and the newest sample ends at the right.
        let stroke_width = rect.height() / (RAINBOW_BANDS + 1) as f32;
        let band_spacing = stroke_width + settings.band_gap;
        let wave_height = band_spacing * 0.85;

        for band in (0..RAINBOW_BANDS).rev() {
            let points: Vec<Pos2> = history
                .iter()
                .enumerate()
                .map(|(column, snapshot)| {
                    let history_column = history_size - history.len() + column;
                    let x = rect.left()
                        + rect.width() * history_column as f32
                            / history_size.saturating_sub(1).max(1) as f32;
                    let baseline = rect.top() + band_spacing * (band + 1) as f32;
                    let displacement = snapshot[band].clamp(0.0, 1.0) * wave_height;
                    if column as f32 > (RAINBOW_BANDS as f32 / 2.0) {
                        Pos2::new(x, baseline - displacement)
                    } else {
                        Pos2::new(x, baseline + displacement)
                    }
                })
                .collect();

            if points.len() > 1 {
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(stroke_width, rainbow_color(band)),
                ));
            } else if let Some(point) = points.first() {
                painter.circle_filled(*point, stroke_width / 2.0, rainbow_color(band));
            }
        }
    }

    fn default_settings() -> VisualisationSettings {
        VisualisationSettings {
            visualiser: VisualisationType::Nyan,
            bands: 6,
            band_gap: 0.0,
            ..VisualisationSettings::default()
        }
    }
}

fn to_rainbow_bands(bands: &[f32]) -> [f32; RAINBOW_BANDS] {
    let mut result = [0.0; RAINBOW_BANDS];
    let chunk_size = bands.len().div_ceil(RAINBOW_BANDS).max(1);

    for (rainbow_band, output) in result.iter_mut().enumerate() {
        let start = rainbow_band * chunk_size;
        let end = (start + chunk_size).min(bands.len());
        if start < end {
            *output = bands[start..end].iter().copied().sum::<f32>() / (end - start) as f32;
        }
    }

    result
}

fn rainbow_color(band: usize) -> Color32 {
    // Clementine uses evenly spaced HSV colours around the rainbow.
    hsv_to_rgb(band as f32 / RAINBOW_BANDS as f32, 1.0, 1.0)
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> Color32 {
    let h = hue.fract() * 6.0;
    let sector = h.floor() as u32;
    let fraction = h - sector as f32;
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * fraction);
    let t = value * (1.0 - saturation * (1.0 - fraction));
    let (r, g, b) = match sector % 6 {
        0 => (value, t, p),
        1 => (q, value, p),
        2 => (p, value, t),
        3 => (p, q, value),
        4 => (t, p, value),
        _ => (value, p, q),
    };

    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}
