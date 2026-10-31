use crate::profiling::{profile_function, profile_scope};
use crate::visualisations::{MAX_BANDS, Visualisation, VisualisationSettings, VisualisationType};
use eframe::emath::Vec2;
use egui::ViewportCommand::InnerSize;
use egui::{Color32, Ui};
use serde::{Deserialize, Serialize};

/// Maximum number of vertical segments per bar.
const MAX_SEGMENTS: usize = 256;
/// Upper bound for gaps (in points).
const MAX_GAP: f32 = 64.0;

const MAX_WINDOW_SIZE: f32 = 8192.0;
/// Colour mapping strategy for the vertical segments in a segmented spectrum bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColorMapping {
    /// Classic 3-tier LED meter: Green (bottom 0.0..0.6) -> Yellow (mid 0.6..0.85) -> Red (top 0.85..1.0)
    LedDefault,
    /// Linear gradient across sorted colour stops: `(position 0.0..=1.0, Color32)`
    Gradient(Vec<(f32, Color32)>),
    /// Stepped threshold colours: `(threshold 0.0..=1.0, Color32)`
    Stepped(Vec<(f32, Color32)>),
    /// HSV rainbow / hue mapping
    Rainbow,
    /// Single solid colour for all segments
    Solid(Color32),
}

impl PartialEq for ColorMapping {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::LedDefault, Self::LedDefault) => true,
            (Self::Gradient(a), Self::Gradient(b)) => a == b,
            (Self::Stepped(a), Self::Stepped(b)) => a == b,
            (Self::Rainbow, Self::Rainbow) => true,
            (Self::Solid(a), Self::Solid(b)) => a == b,
            _ => false,
        }
    }
}

impl Default for ColorMapping {
    fn default() -> Self {
        ColorMapping::LedDefault
    }
}

impl ColorMapping {
    /// Creates the default 3-tier LED meter mapping (Green, Yellow, Red).
    pub fn led_default() -> Self {
        ColorMapping::LedDefault
    }

    /// Creates a smooth linear gradient across arbitrary colour stops.
    pub fn gradient(mut stops: Vec<(f32, Color32)>) -> Self {
        stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        ColorMapping::Gradient(stops)
    }

    /// Creates a 2-stop linear gradient from bottom to top.
    pub fn two_stop_gradient(bottom: Color32, top: Color32) -> Self {
        Self::gradient(vec![(0.0, bottom), (1.0, top)])
    }

    /// Creates a 3-stop linear gradient (bottom, middle, top).
    pub fn three_stop_gradient(bottom: Color32, middle: Color32, top: Color32) -> Self {
        Self::gradient(vec![(0.0, bottom), (0.5, middle), (1.0, top)])
    }

    /// Creates a stepped threshold colour mapping.
    pub fn stepped(mut steps: Vec<(f32, Color32)>) -> Self {
        steps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        ColorMapping::Stepped(steps)
    }

    /// Creates an HSV rainbow colour mapping.
    pub fn rainbow() -> Self {
        ColorMapping::Rainbow
    }

    /// Creates a solid single colour mapping.
    pub fn solid(color: Color32) -> Self {
        ColorMapping::Solid(color)
    }

    /// Resolves the colour for a normalised height position (0.0 at bottom, 1.0 at top).
    pub fn get_color(&self, normalized_height: f32) -> Color32 {
        let t = normalized_height.clamp(0.0, 1.0);
        match self {
            ColorMapping::LedDefault => {
                if t < 0.6 {
                    Color32::from_rgb(0, 220, 50)
                } else if t < 0.85 {
                    Color32::from_rgb(255, 200, 0)
                } else {
                    Color32::from_rgb(255, 30, 30)
                }
            }
            ColorMapping::Gradient(stops) => {
                if stops.is_empty() {
                    return Color32::from_rgb(0, 220, 50);
                }
                if stops.len() == 1 || t <= stops[0].0 {
                    return stops[0].1;
                }
                if t >= stops.last().unwrap().0 {
                    return stops.last().unwrap().1;
                }
                for window in stops.windows(2) {
                    let (p0, c0) = window[0];
                    let (p1, c1) = window[1];
                    if t >= p0 && t <= p1 {
                        let span = p1 - p0;
                        let factor = if span <= 0.0 { 0.0 } else { (t - p0) / span };
                        let r = c0.r() as f32 + (c1.r() as f32 - c0.r() as f32) * factor;
                        let g = c0.g() as f32 + (c1.g() as f32 - c0.g() as f32) * factor;
                        let b = c0.b() as f32 + (c1.b() as f32 - c0.b() as f32) * factor;
                        let a = c0.a() as f32 + (c1.a() as f32 - c0.a() as f32) * factor;
                        return Color32::from_rgba_unmultiplied(
                            r.round() as u8,
                            g.round() as u8,
                            b.round() as u8,
                            a.round() as u8,
                        );
                    }
                }
                stops.last().unwrap().1
            }
            ColorMapping::Stepped(steps) => {
                if steps.is_empty() {
                    return Color32::from_rgb(0, 220, 50);
                }
                let mut current_color = steps[0].1;
                for &(threshold, color) in steps {
                    if t >= threshold {
                        current_color = color;
                    }
                }
                current_color
            }
            ColorMapping::Rainbow => {
                let hue = (1.0 - t) * 240.0;
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
            ColorMapping::Solid(color) => *color,
        }
    }

    /// Human-readable name of the active mapping strategy.
    fn kind_name(&self) -> &'static str {
        match self {
            ColorMapping::LedDefault => "LED (green / yellow / red)",
            ColorMapping::Gradient(_) => "Gradient",
            ColorMapping::Stepped(_) => "Stepped",
            ColorMapping::Rainbow => "Rainbow",
            ColorMapping::Solid(_) => "Solid",
        }
    }

    /// Clamps stop positions into `0.0..=1.0` and keeps them sorted.
    pub fn sanitize(&mut self) {
        if let ColorMapping::Gradient(stops) | ColorMapping::Stepped(stops) = self {
            sanitize_stops(stops);
        }
    }

    /// Draws the colour mapping editor. Returns `true` if the mapping was changed.
    ///
    /// The `gradient_stops` / `stepped_stops` caches hold the stops of each stop-based mapping so
    /// that switching to another mode and back does not discard the configured stops. They are
    /// seeded when switching *to* a stop-based mode and synchronised after every edit.
    pub fn settings_ui(
        &mut self,
        ui: &mut Ui,
        gradient_stops: &mut Vec<(f32, Color32)>,
        stepped_stops: &mut Vec<(f32, Color32)>,
    ) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Colour mapping");
            egui::ComboBox::from_id_salt("segmented_spectrum_bars_color_mapping")
                .selected_text(self.kind_name())
                .show_ui(ui, |ui| {
                    let options = [
                        ColorMapping::LedDefault,
                        ColorMapping::Rainbow,
                        ColorMapping::Solid(Color32::from_rgb(0, 220, 50)),
                        ColorMapping::gradient(clamped_stops(
                            gradient_stops,
                            &default_gradient_stops(),
                        )),
                        ColorMapping::stepped(clamped_stops(
                            stepped_stops,
                            &default_stepped_stops(),
                        )),
                    ];
                    for option in options {
                        let selected =
                            std::mem::discriminant(self) == std::mem::discriminant(&option);
                        if ui.selectable_label(selected, option.kind_name()).clicked() && !selected
                        {
                            *self = option;
                            changed = true;
                        }
                    }
                });
        });

        match self {
            ColorMapping::LedDefault | ColorMapping::Rainbow => {}
            ColorMapping::Solid(color) => {
                ui.horizontal(|ui| {
                    ui.label("Colour");
                    changed |= ui.color_edit_button_srgba(color).changed();
                });
            }
            ColorMapping::Gradient(stops) | ColorMapping::Stepped(stops) => {
                changed |= stops_ui(ui, stops);
            }
        }

        if changed {
            // Keep the persistent caches in sync with the active mapping.
            match self {
                ColorMapping::Gradient(stops) => *gradient_stops = stops.clone(),
                ColorMapping::Stepped(stops) => *stepped_stops = stops.clone(),
                _ => {}
            }
        }

        changed
    }

    /// Resolves the colour for a segment index out of a total number of segments.
    pub fn get_color_for_segment(&self, segment_index: usize, total_segments: usize) -> Color32 {
        if total_segments <= 1 {
            self.get_color(0.0)
        } else {
            let normalized = segment_index as f32 / (total_segments - 1) as f32;
            self.get_color(normalized)
        }
    }
}

/// Editor for the `(position, colour)` stops of a gradient or stepped mapping.
fn stops_ui(ui: &mut Ui, stops: &mut Vec<(f32, Color32)>) -> bool {
    let mut changed = false;
    let mut remove = None;
    let can_remove = stops.len() > 1;

    for (index, (position, color)) in stops.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("Stop {}", index + 1));
            changed |= ui
                .add(
                    egui::DragValue::new(position)
                        .range(0.0..=1.0)
                        .speed(0.01)
                        .fixed_decimals(2),
                )
                .changed();
            changed |= ui.color_edit_button_srgba(color).changed();
            if ui
                .add_enabled(can_remove, egui::Button::new("Remove").small())
                .clicked()
            {
                remove = Some(index);
            }
        });
    }

    if let Some(index) = remove {
        stops.remove(index);
        changed = true;
    }

    if ui.small_button("Add stop").clicked() {
        let (position, color) = stops.last().copied().unwrap_or((0.0, Color32::WHITE));
        stops.push((position, color));
        changed = true;
    }

    changed
}

/// Default stops for the gradient colour mapping.
fn default_gradient_stops() -> Vec<(f32, Color32)> {
    vec![
        (0.0, Color32::from_rgb(0, 220, 50)),
        (1.0, Color32::from_rgb(255, 30, 30)),
    ]
}

/// Default stops for the stepped colour mapping.
fn default_stepped_stops() -> Vec<(f32, Color32)> {
    vec![
        (0.0, Color32::from_rgb(0, 220, 50)),
        (0.6, Color32::from_rgb(255, 200, 0)),
        (0.85, Color32::from_rgb(255, 30, 30)),
    ]
}

/// Clamps stop positions into `0.0..=1.0` and keeps them sorted in place.
fn sanitize_stops(stops: &mut Vec<(f32, Color32)>) {
    for stop in stops.iter_mut() {
        stop.0 = stop.0.clamp(0.0, 1.0);
    }
    stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
}

/// Returns the cached stops, or the given defaults when the cache is empty. The result is a
/// freshly sanitised copy so the active `ColorMapping` never borrows the cache.
fn clamped_stops(cache: &[(f32, Color32)], defaults: &[(f32, Color32)]) -> Vec<(f32, Color32)> {
    let mut stops = if cache.is_empty() {
        defaults.to_vec()
    } else {
        cache.to_vec()
    };
    sanitize_stops(&mut stops);
    stops
}

/// Segmented spectrum bars visualisation.
///
/// Divides each frequency band into vertically stacked discrete segments (LED bars).
/// The horizontal gap between bars and vertical gap between segments, as well as the
/// colour mapping strategy, are all configurable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SegmentedSpectrumBarsSettings {
    /// Vertical gap between segments within each bar.
    pub vertical_gap: f32,
    /// Total number of vertical segments per bar.
    pub segment_count: usize,
    /// Corner radius for each segment rectangle.
    pub corner_radius: u8,
    /// Colour mapping strategy for segments.
    pub color_mapping: ColorMapping,
    /// Whether to render inactive (unlit) segments in the background.
    pub draw_inactive_segments: bool,
    /// Color for inactive segments when `draw_inactive_segments` is enabled.
    pub inactive_color: Color32,
    pub window_width: f32,
    pub window_height: f32,
    pub bands: usize,
    pub band_gap: f32,
    /// The `(position, colour)` stops of the gradient mapping, kept separately from the active
    /// [`Self::color_mapping`] so they survive switching to another colour-mapping mode and back.
    pub gradient_stops: Vec<(f32, Color32)>,
    /// The `(threshold, colour)` stops of the stepped mapping, kept separately from the active
    /// [`Self::color_mapping`] so they survive switching to another colour-mapping mode and back.
    pub stepped_stops: Vec<(f32, Color32)>,
}

impl Default for SegmentedSpectrumBarsSettings {
    fn default() -> Self {
        Self {
            band_gap: 2.0,
            vertical_gap: 2.0,
            segment_count: 32,
            corner_radius: 0,
            color_mapping: ColorMapping::default(),
            draw_inactive_segments: false,
            inactive_color: Color32::from_rgba_unmultiplied(40, 40, 40, 40),
            window_width: 800.0,
            window_height: 300.0,
            bands: 32,
            // Default stop lists. They are kept in sync with the active mapping while it is in
            // use, so switching to another colour-mapping mode and back keeps the configured stops.
            gradient_stops: vec![
                (0.0, Color32::from_rgb(0, 220, 50)),
                (1.0, Color32::from_rgb(255, 30, 30)),
            ],
            stepped_stops: vec![
                (0.0, Color32::from_rgb(0, 220, 50)),
                (0.6, Color32::from_rgb(255, 200, 0)),
                (0.85, Color32::from_rgb(255, 30, 30)),
            ],
        }
    }
}

impl SegmentedSpectrumBarsSettings {
    /// Creates a new `SegmentedSpectrumBars` instance with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }
    pub fn sanitize(&mut self) {
        self.band_gap = self.band_gap.clamp(0.0, MAX_GAP);
        self.vertical_gap = self.vertical_gap.clamp(0.0, MAX_GAP);
        self.segment_count = self.segment_count.clamp(1, MAX_SEGMENTS);
        self.window_width = self.window_width.clamp(0.0, MAX_WINDOW_SIZE);
        self.window_height = self.window_height.clamp(0.0, MAX_WINDOW_SIZE);
        self.color_mapping.sanitize();
        sanitize_stops(&mut self.gradient_stops);
        sanitize_stops(&mut self.stepped_stops);
    }

    /// Draws the settings editor. Returns `true` if any value was changed.
    pub fn settings_ui(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        egui::Grid::new("segmented_spectrum_bars_settings")
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
                if ui.button("↩️").clicked() {
                    self.window_width = SegmentedSpectrumBarsSettings::default().window_width;
                }
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
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.band_gap)
                            .range(0.0..=64.0)
                            .speed(1),
                    )
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

                ui.label("Segments per bar");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.segment_count)
                            .range(1..=MAX_SEGMENTS)
                            .speed(1),
                    )
                    .changed();
                ui.end_row();

                ui.label("Vertical gap");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.vertical_gap)
                            .range(0.0..=MAX_GAP)
                            .speed(1),
                    )
                    .changed();
                ui.end_row();

                // ui.label("Custom horizontal gap");
                // let mut custom_gap = self.horizontal_gap.is_some();
                // if ui.checkbox(&mut custom_gap, "").changed() {
                //     self.horizontal_gap = custom_gap.then_some(self.horizontal_gap.unwrap_or(2.0));
                //     changed = true;
                // }
                // ui.end_row();

                ui.label("Band gap");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.band_gap)
                            .range(0.0..=MAX_GAP)
                            .speed(1),
                    )
                    .changed();
                ui.end_row();

                ui.label("Corner radius");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.corner_radius)
                            .range(0..=32)
                            .speed(1),
                    )
                    .changed();
                ui.end_row();

                ui.label("Draw inactive segments");
                changed |= ui.checkbox(&mut self.draw_inactive_segments, "").changed();
                ui.end_row();

                if self.draw_inactive_segments {
                    ui.label("Inactive colour");
                    changed |= ui
                        .color_edit_button_srgba(&mut self.inactive_color)
                        .changed();
                    ui.end_row();
                }
            });

        ui.separator();
        changed |=
            self.color_mapping
                .settings_ui(ui, &mut self.gradient_stops, &mut self.stepped_stops);
        changed
    }
}
pub struct SegmentedSpectrumBars(pub SegmentedSpectrumBarsSettings);
impl Visualisation for SegmentedSpectrumBars {
    fn draw(&self, ui: &mut Ui, bands: &[f32], settings: &VisualisationSettings) {
        profile_function!();
        let own = &self.0;
        let window_size = Vec2::new(own.window_width, own.window_height);
        ui.ctx().send_viewport_cmd(InnerSize(window_size));
        let (rect, _) = ui.allocate_exact_size(window_size, egui::Sense::hover());
        let painter = ui.painter_at(rect);

        if bands.is_empty() || rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }

        let band_count = bands.len();
        let total_h_gap = own.band_gap * band_count.saturating_sub(1) as f32;
        let bar_width = ((rect.width() - total_h_gap) / band_count as f32).max(1.0);

        let num_segments = own.segment_count.max(1);
        let total_v_gap = own.vertical_gap * num_segments.saturating_sub(1) as f32;

        let (seg_height, actual_v_gap) = if rect.height() > total_v_gap {
            let seg_h = (rect.height() - total_v_gap) / num_segments as f32;
            (seg_h, own.vertical_gap)
        } else {
            let seg_h = (rect.height() / num_segments as f32 * 0.8).max(1.0);
            let gap = (rect.height() - seg_h * num_segments as f32)
                / num_segments.saturating_sub(1).max(1) as f32;
            (seg_h, gap.max(0.0))
        };

        {
            profile_scope!("segments");
            for (i, &level) in bands.iter().enumerate() {
                let x = rect.left() + i as f32 * (bar_width + own.band_gap);
                let level_clamped = (level * settings.gain).clamp(0.0, 1.0);
                let active_count = if level_clamped <= 0.0 {
                    0
                } else {
                    ((level_clamped * num_segments as f32).round() as usize).min(num_segments)
                };
                for s in 0..num_segments {
                    let is_active = s < active_count;
                    if !is_active && !own.draw_inactive_segments {
                        continue;
                    }

                    let seg_bottom = rect.bottom() - s as f32 * (seg_height + actual_v_gap);
                    let seg_top = seg_bottom - seg_height;
                    let seg_rect = egui::Rect::from_min_max(
                        egui::pos2(x, seg_top),
                        egui::pos2(x + bar_width, seg_bottom),
                    );

                    let color = if is_active {
                        own.color_mapping.get_color_for_segment(s, num_segments)
                    } else {
                        own.inactive_color
                    };

                    painter.rect_filled(
                        seg_rect,
                        egui::CornerRadius::same(own.corner_radius),
                        color,
                    );
                }
            }
        }
    }

    fn default_settings() -> VisualisationSettings {
        VisualisationSettings {
            visualiser: VisualisationType::SegmentedSpectrumBars(
                SegmentedSpectrumBarsSettings::default(),
            ),
            band_gap: 2.0,
            ..VisualisationSettings::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_mapping_led_default() {
        let mapping = ColorMapping::LedDefault;
        // Low: Green
        assert_eq!(mapping.get_color(0.1), Color32::from_rgb(0, 220, 50));
        assert_eq!(mapping.get_color(0.59), Color32::from_rgb(0, 220, 50));
        // Mid: Yellow
        assert_eq!(mapping.get_color(0.6), Color32::from_rgb(255, 200, 0));
        assert_eq!(mapping.get_color(0.84), Color32::from_rgb(255, 200, 0));
        // High: Red
        assert_eq!(mapping.get_color(0.85), Color32::from_rgb(255, 30, 30));
        assert_eq!(mapping.get_color(1.0), Color32::from_rgb(255, 30, 30));
    }

    #[test]
    fn test_color_mapping_gradient() {
        let mapping = ColorMapping::two_stop_gradient(
            Color32::from_rgb(0, 0, 0),
            Color32::from_rgb(100, 200, 50),
        );
        assert_eq!(mapping.get_color(0.0), Color32::from_rgb(0, 0, 0));
        assert_eq!(mapping.get_color(1.0), Color32::from_rgb(100, 200, 50));
        let mid = mapping.get_color(0.5);
        assert_eq!(mid, Color32::from_rgb(50, 100, 25));
    }

    #[test]
    fn test_color_mapping_stepped() {
        let mapping = ColorMapping::stepped(vec![(0.0, Color32::BLUE), (0.5, Color32::WHITE)]);
        assert_eq!(mapping.get_color(0.2), Color32::BLUE);
        assert_eq!(mapping.get_color(0.5), Color32::WHITE);
        assert_eq!(mapping.get_color(0.9), Color32::WHITE);
    }

    #[test]
    fn test_color_mapping_solid() {
        let mapping = ColorMapping::solid(Color32::RED);
        assert_eq!(mapping.get_color(0.0), Color32::RED);
        assert_eq!(mapping.get_color(0.5), Color32::RED);
        assert_eq!(mapping.get_color(1.0), Color32::RED);
    }

    #[test]
    fn test_draw_with_egui() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let bars = VisualisationType::SegmentedSpectrumBars(SegmentedSpectrumBarsSettings {
                vertical_gap: 2.0,
                segment_count: 10,
                color_mapping: Default::default(),
                draw_inactive_segments: true,
                inactive_color: Color32::BLACK,
                band_gap: 2.0,
                ..Default::default()
            });

            let settings = VisualisationSettings {
                visualiser: VisualisationType::SegmentedSpectrumBars(
                    SegmentedSpectrumBarsSettings::default(),
                ),
                gain: 1.0,
                smoothing: 0.0,
                window_size: Vec2::new(400.0, 200.0),
                band_gap: 2.0,
                ..VisualisationSettings::default()
            };

            let band_data = vec![0.0, 0.25, 0.5, 0.75, 1.0, 0.1, 0.9, 0.0];
            bars.draw(ui, &band_data, &settings);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn sanitize_stops_clamps_and_sorts() {
        let mut stops = vec![
            (2.0, Color32::BLUE),
            (-1.0, Color32::RED),
            (0.5, Color32::WHITE),
            (0.5, Color32::BLACK),
        ];
        sanitize_stops(&mut stops);
        let positions: Vec<f32> = stops.iter().map(|s| s.0).collect();
        assert_eq!(positions, vec![0.0, 0.5, 0.5, 1.0]);
        assert_eq!(stops[0].1, Color32::RED);
        assert_eq!(stops[3].1, Color32::BLUE);
    }

    #[test]
    fn clamped_stops_uses_cache_when_present_and_defaults_when_empty() {
        let cache = vec![(0.25, Color32::GREEN), (0.75, Color32::BLUE)];
        let defaults = default_stepped_stops();

        let from_cache = clamped_stops(&cache, &defaults);
        assert_eq!(from_cache, cache);

        let from_defaults = clamped_stops(&[], &defaults);
        assert_eq!(from_defaults, defaults);
    }

    #[test]
    fn stepped_stops_survive_round_trip_and_mode_change() {
        // Configure custom stepped stops and store them in the settings struct.
        let mut settings = SegmentedSpectrumBarsSettings::default();
        settings.color_mapping = ColorMapping::stepped(vec![
            (0.0, Color32::GREEN),
            (0.5, Color32::YELLOW),
            (0.9, Color32::RED),
        ]);
        settings.stepped_stops = vec![
            (0.0, Color32::GREEN),
            (0.5, Color32::YELLOW),
            (0.9, Color32::RED),
        ];

        // Simulate switching to another mode: color_mapping changes, the cache stays.
        settings.color_mapping = ColorMapping::Rainbow;
        assert_eq!(settings.stepped_stops[1].1, Color32::YELLOW);

        // Switching back to Stepped reuses the cached stops rather than discarding them.
        settings.color_mapping = ColorMapping::Stepped(settings.stepped_stops.clone());
        assert_eq!(
            settings.color_mapping,
            ColorMapping::Stepped(vec![
                (0.0, Color32::GREEN),
                (0.5, Color32::YELLOW),
                (0.9, Color32::RED),
            ])
        );

        // The caches are serialised too, so they survive an app restart.
        let json = serde_json::to_string(&settings).unwrap();
        let restored: SegmentedSpectrumBarsSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.stepped_stops, settings.stepped_stops);
    }
}
