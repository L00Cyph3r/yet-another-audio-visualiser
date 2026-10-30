use std::collections::VecDeque;

use crate::visualisations::{Visualisation, VisualisationSettings, VisualisationType};
use egui::ViewportCommand::InnerSize;
use egui::{Color32, ColorImage, Pos2, Rect, Stroke, TextureHandle, TextureOptions, Ui, Vec2};
use serde::{Deserialize, Serialize};

const RAINBOW_BANDS: usize = 6;

/// The Nyan Cat sprite, embedded in the executable.
const CAT_PNG: &[u8] = include_bytes!("../../assets/img/nyan.png");
/// Every "pixel" of the pixel art is a square of this many pixels in `nyan.png`.
const SOURCE_PIXEL_SIZE: u32 = 35;

/// Where the rainbow leaves the cat, as a fraction of the sprite's size. The rainbow starts
/// inside the pop-tart (horizontally) and is centred on it (vertically); the cat is drawn on
/// top, so only the part that sticks out on the left is visible.
const RAINBOW_ORIGIN_X: f32 = 0.3;
const RAINBOW_CENTER_Y: f32 = 0.43;

/// Distance (in points) between two recorded spectrum snapshots in the trail.
const SAMPLE_SPACING: f64 = 4.0;

const MAX_AMPLITUDE_FACTOR: f32 = 100.0;
const MAX_LINE_WIDTH: f32 = 200.0;
const MAX_LINE_GAP: f32 = 100.0;
const MAX_SPEED: f32 = 2000.0;
const MAX_WINDOW_SIZE: f32 = 8192.0;

/// The classic rainbow, from the top line to the bottom line.
const RAINBOW: [Color32; RAINBOW_BANDS] = [
    Color32::from_rgb(255, 0, 0),
    Color32::from_rgb(255, 153, 0),
    Color32::from_rgb(255, 255, 0),
    Color32::from_rgb(51, 255, 0),
    Color32::from_rgb(0, 153, 255),
    Color32::from_rgb(102, 51, 255),
];

/// Individual settings of the [`Nyan`] visualisation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Copy)]
#[serde(default)]
pub struct NyanSettings {
    pub amplitude_factor: f32,
    pub line_width: f32,
    pub line_gap: f32,
    pub speed: f32,
    pub window_width: f32,
    pub window_height: f32,
}

impl Default for NyanSettings {
    fn default() -> Self {
        Self {
            amplitude_factor: 4.0,
            line_width: 12.0,
            line_gap: 0.0,
            speed: 200.0,
            window_width: 1600.0,
            window_height: 200.0,
        }
    }
}

impl NyanSettings {
    pub fn sanitize(&mut self) {
        self.amplitude_factor = self.amplitude_factor.clamp(0.01, MAX_AMPLITUDE_FACTOR);
        self.line_width = self.line_width.clamp(1.0, MAX_LINE_WIDTH);
        self.line_gap = self.line_gap.clamp(0.0, MAX_LINE_GAP);
        self.speed = self.speed.clamp(0.0, MAX_SPEED);
        self.window_width = self.window_width.clamp(0.0, MAX_WINDOW_SIZE);
        self.window_height = self.window_height.clamp(0.0, MAX_WINDOW_SIZE);
    }

    /// The area the visualisation is drawn into: the configured size, centred in `window`
    /// and limited to it. A size of `0` uses everything the window offers in that direction.
    fn visualisation_area(&self, window: Rect) -> Rect {
        let fit = |configured: f32, available: f32| {
            if configured > 0.0 {
                configured.min(available)
            } else {
                available
            }
        };
        let size = egui::vec2(
            fit(self.window_width, window.width()),
            fit(self.window_height, window.height()),
        );
        Rect::from_center_size(window.center(), size)
    }

    /// Draws the settings editor. Returns `true` if any value was changed.
    pub fn settings_ui(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        egui::Grid::new("nyan_settings")
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
                ui.label("Amplitude factor");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.amplitude_factor)
                            .range(1.0..=MAX_AMPLITUDE_FACTOR)
                            .speed(0.1),
                    )
                    .changed();
                ui.end_row();

                ui.label("Line width");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.line_width)
                            .range(1.0..=MAX_LINE_WIDTH)
                            .speed(0.5)
                            .suffix(" px"),
                    )
                    .changed();
                ui.end_row();

                ui.label("Line gap");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.line_gap)
                            .range(0.0..=MAX_LINE_GAP)
                            .speed(0.5)
                            .suffix(" px"),
                    )
                    .changed();
                ui.end_row();

                ui.label("Speed");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.speed)
                            .range(0.0..=MAX_SPEED)
                            .speed(2.0)
                            .suffix(" px/s"),
                    )
                    .changed();
                ui.end_row();
            });
        changed
    }
}

/// One recorded snapshot of the spectrum.
#[derive(Debug, Clone, Copy)]
struct Sample {
    /// Scroll distance at the time the snapshot was taken.
    born: f64,
    levels: [f32; RAINBOW_BANDS],
}

/// The scrolling history of the rainbow.
///
/// Time is measured as scroll *distance*, not frames: a snapshot is recorded every
/// [`SAMPLE_SPACING`] points of scrolling, so the speed does not depend on the frame rate and
/// a speed of zero freezes the rainbow.
#[derive(Debug, Clone, Default)]
struct Trail {
    samples: VecDeque<Sample>,
    /// Total distance scrolled so far.
    distance: f64,
    /// Scroll distance of the most recent snapshot.
    last_born: f64,
    /// The latest spectrum; the rainbow's head.
    current: [f32; RAINBOW_BANDS],
}

impl Trail {
    /// Scrolls by `delta` points and records the snapshots that became due. Snapshots that
    /// scrolled further than `max_age` points are dropped.
    fn advance(&mut self, delta: f32, current: [f32; RAINBOW_BANDS], max_age: f32) {
        self.current = current;
        self.distance += f64::from(delta.max(0.0));

        while self.last_born + SAMPLE_SPACING <= self.distance {
            self.last_born += SAMPLE_SPACING;
            self.samples.push_back(Sample {
                born: self.last_born,
                levels: current,
            });
        }

        let limit = f64::from(max_age.max(0.0)) + SAMPLE_SPACING;
        while self
            .samples
            .front()
            .is_some_and(|sample| self.distance - sample.born > limit)
        {
            self.samples.pop_front();
        }
    }
}

pub(crate) struct Nyan(pub NyanSettings);

impl Visualisation for Nyan {
    fn draw(&self, ui: &mut Ui, bands: &[f32], settings: &VisualisationSettings) {
        let (window, _) = ui.allocate_exact_size(settings.window_size, egui::Sense::hover());
        let own = &self.0;
        ui.ctx()
            .send_viewport_cmd(InnerSize(Vec2::new(own.window_width, own.window_height)));
        let rect = own.visualisation_area(window);

        if bands.is_empty() || rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }

        let painter = ui.painter_at(rect);

        let sprite = sprite(ui.ctx());
        let cat_rect = sprite.as_ref().map(|sprite| cat_rect(rect, sprite.size()));
        let (origin_x, center_y) = match cat_rect {
            Some(cat) => (
                cat.left() + RAINBOW_ORIGIN_X * cat.width(),
                cat.top() + RAINBOW_CENTER_Y * cat.height(),
            ),
            None => (rect.right(), rect.center().y),
        };

        // Nyan is recreated by the enum dispatcher each frame, so the history
        // lives in egui's temporary widget state instead of in `Nyan` itself.
        let trail_id = ui.scope_id().with("nyan-rainbow-trail");
        let dt = ui.input(|input| input.stable_dt).clamp(0.0, 0.1);
        let current = to_rainbow_bands(bands);
        let trail = ui.ctx().data_mut(|data| {
            let trail = data.get_temp_mut_or_default::<Trail>(trail_id);
            trail.advance(dt * own.speed, current, origin_x - rect.left());
            trail.clone()
        });

        // Six parallel, coloured lines, stacked around the vertical centre. Each line is
        // displaced by its part of the spectrum: the top half moves up and the bottom half
        // moves down, away from the centre.
        // The newest snapshot is at the cat; older ones scroll away to the left.
        let pitch = own.line_width + own.line_gap;
        let amplitude = own.line_width * settings.gain * own.amplitude_factor;
        let top_line_y = center_y - pitch * (RAINBOW_BANDS - 1) as f32 / 2.0;

        // Draw from the bottom up so the top lines end up in front.
        for band in (0..RAINBOW_BANDS).rev() {
            let baseline = top_line_y + pitch * band as f32;
            let direction = displacement_direction(band);
            let to_point = |x: f32, level: f32| {
                Pos2::new(x, baseline + direction * level.clamp(0.0, 1.0) * amplitude)
            };

            let mut points: Vec<Pos2> = trail
                .samples
                .iter()
                .map(|sample| {
                    let age = (trail.distance - sample.born) as f32;
                    to_point(origin_x - age, sample.levels[band])
                })
                .collect();
            points.push(to_point(origin_x, trail.current[band]));

            if points.len() > 1 {
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(own.line_width, RAINBOW[band]),
                ));
            }
        }

        if let (Some(sprite), Some(cat_rect)) = (sprite, cat_rect) {
            painter.image(
                sprite.id(),
                cat_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }

    fn default_settings() -> VisualisationSettings {
        VisualisationSettings {
            visualiser: VisualisationType::Nyan(NyanSettings::default()),
            band_gap: 0.0,
            ..VisualisationSettings::default()
        }
    }
}

/// The direction (in screen coordinates, where `y` grows downwards) in which a line moves
/// when its level rises: the top half of the rainbow moves up (`-1`), the bottom half moves
/// down (`+1`).
fn displacement_direction(band: usize) -> f32 {
    const BANDS_HALF: usize = RAINBOW_BANDS / 2;

    if band < BANDS_HALF {
        // Top half (bands 0-2): moves upward (negative y); the outermost band moves the most.
        -(((BANDS_HALF - band) as f32) / BANDS_HALF as f32)
    } else {
        // Bottom half (bands 3-5): moves downward (positive y); the outermost band moves the most.
        ((band - BANDS_HALF + 1) as f32) / BANDS_HALF as f32
    }
}

/// Where to draw the cat: at the right edge, vertically centred, as large as fits.
///
/// The size is snapped to a whole number of screen points per sprite pixel, which keeps
/// the pixel art crisp.
fn cat_rect(area: Rect, sprite_size: [usize; 2]) -> Rect {
    let (width, height) = (sprite_size[0] as f32, sprite_size[1] as f32);
    let fit = (area.height() * 0.9 / height).min(area.width() * 0.5 / width);
    let pixel = if fit >= 1.0 { fit.floor() } else { fit };
    let size = egui::vec2(width * pixel, height * pixel);
    let right = area.right() - area.width() * 0.03;
    Rect::from_min_size(
        Pos2::new(right - size.x, area.center().y - size.y / 2.0),
        size,
    )
}

/// The cat's texture, loaded on first use and cached in the egui context.
fn sprite(ctx: &egui::Context) -> Option<TextureHandle> {
    let id = egui::Id::unique("nyan-cat-sprite");
    if let Some(cached) = ctx.data(|data| data.get_temp::<Option<TextureHandle>>(id)) {
        return cached;
    }

    let texture =
        sprite_image().map(|image| ctx.load_texture("nyan-cat", image, TextureOptions::NEAREST));
    if texture.is_none() {
        log::error!("Could not load the embedded Nyan Cat sprite");
    }
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}

/// Decodes the embedded sprite, trims the transparent border and reduces it to one pixel per
/// pixel of the pixel art (the PNG is a 35x magnified version of it).
fn sprite_image() -> Option<ColorImage> {
    let rgba = image::load_from_memory_with_format(CAT_PNG, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();

    let (mut min_x, mut min_y, mut max_x, mut max_y) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, pixel) in rgba.enumerate_pixels() {
        if pixel[3] > 0 {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if min_x > max_x || min_y > max_y {
        return None;
    }

    let (width, height) = (max_x - min_x + 1, max_y - min_y + 1);
    let cropped = image::imageops::crop_imm(&rgba, min_x, min_y, width, height).to_image();

    if width % SOURCE_PIXEL_SIZE != 0 || height % SOURCE_PIXEL_SIZE != 0 {
        // Not the magnified pixel art we expect; use the image as it is.
        return Some(ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            cropped.as_raw(),
        ));
    }

    let (columns, rows) = (width / SOURCE_PIXEL_SIZE, height / SOURCE_PIXEL_SIZE);
    let mut pixels = Vec::with_capacity((columns * rows) as usize);
    for row in 0..rows {
        for column in 0..columns {
            let source = cropped.get_pixel(
                column * SOURCE_PIXEL_SIZE + SOURCE_PIXEL_SIZE / 2,
                row * SOURCE_PIXEL_SIZE + SOURCE_PIXEL_SIZE / 2,
            );
            pixels.push(Color32::from_rgba_unmultiplied(
                source[0], source[1], source[2], source[3],
            ));
        }
    }
    Some(ColorImage::new([columns as usize, rows as usize], pixels))
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

#[cfg(test)]
mod tests {
    use super::*;

    const LEVELS: [f32; RAINBOW_BANDS] = [0.5; RAINBOW_BANDS];

    #[test]
    fn sprite_is_reduced_to_its_pixel_art_size() {
        let image = sprite_image().expect("sprite should decode");
        assert_eq!(image.size, [32, 20]);
        // The cat is opaque in the middle and transparent in the corners.
        assert_eq!(image.pixels[10 * 32 + 16].a(), 255);
        assert_eq!(image.pixels[0].a(), 0);
    }

    #[test]
    fn trail_records_one_sample_per_spacing() {
        let mut trail = Trail::default();
        trail.advance(0.0, LEVELS, 1000.0);
        assert!(trail.samples.is_empty());

        // 4 points per sample: 10 points => 2 samples, 2 points left over.
        trail.advance(10.0, LEVELS, 1000.0);
        assert_eq!(trail.samples.len(), 2);

        // The leftover carries over, so small steps still add up.
        trail.advance(2.0, LEVELS, 1000.0);
        assert_eq!(trail.samples.len(), 3);
    }

    #[test]
    fn trail_speed_does_not_depend_on_frame_rate() {
        let mut slow_frames = Trail::default();
        let mut fast_frames = Trail::default();
        for _ in 0..10 {
            slow_frames.advance(10.0, LEVELS, 1000.0);
        }
        for _ in 0..100 {
            fast_frames.advance(1.0, LEVELS, 1000.0);
        }
        assert_eq!(slow_frames.samples.len(), fast_frames.samples.len());
    }

    #[test]
    fn trail_with_zero_speed_stands_still() {
        let mut trail = Trail::default();
        for _ in 0..100 {
            trail.advance(0.0, LEVELS, 1000.0);
        }
        assert!(trail.samples.is_empty());
        assert_eq!(trail.distance, 0.0);
    }

    #[test]
    fn trail_drops_samples_that_scrolled_out_of_view() {
        let mut trail = Trail::default();
        trail.advance(1000.0, LEVELS, 100.0);
        assert!(!trail.samples.is_empty());
        assert!(
            trail
                .samples
                .iter()
                .all(|s| trail.distance - s.born <= 100.0 + SAMPLE_SPACING)
        );
    }

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        let mut settings = NyanSettings {
            amplitude_factor: 0.0,
            line_width: 0.0,
            line_gap: -4.0,
            speed: 1.0e9,
            window_width: -10.0,
            window_height: 1.0e9,
        };
        settings.sanitize();
        assert_eq!(settings.line_width, 1.0);
        assert_eq!(settings.line_gap, 0.0);
        assert_eq!(settings.speed, MAX_SPEED);
        assert_eq!(settings.window_width, 0.0);
        assert_eq!(settings.window_height, MAX_WINDOW_SIZE);
    }

    #[test]
    fn visualisation_area_defaults_to_the_whole_window() {
        let window = Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(600.0, 200.0));
        assert_eq!(NyanSettings::default().visualisation_area(window), window);
    }

    #[test]
    fn visualisation_area_is_centred_and_limited_to_the_window() {
        let window = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 300.0));

        let smaller = NyanSettings {
            window_width: 400.0,
            window_height: 100.0,
            ..NyanSettings::default()
        };
        let area = smaller.visualisation_area(window);
        assert_eq!(area.size(), egui::vec2(400.0, 100.0));
        assert_eq!(area.center(), window.center());

        // One axis can be configured while the other follows the window.
        let only_width = NyanSettings {
            window_width: 200.0,
            ..NyanSettings::default()
        };
        assert_eq!(
            only_width.visualisation_area(window).size(),
            egui::vec2(200.0, 200.0)
        );

        // A size larger than the window is cut down to the window.
        let larger = NyanSettings {
            window_width: 5000.0,
            window_height: 5000.0,
            ..NyanSettings::default()
        };
        assert_eq!(larger.visualisation_area(window), window);
    }

    #[test]
    fn cat_fits_into_the_area_and_sits_on_the_right() {
        let area = Rect::from_min_size(Pos2::ZERO, egui::vec2(1024.0, 512.0));
        let cat = cat_rect(area, [32, 20]);
        assert!(area.contains_rect(cat));
        assert!(cat.center().x > area.center().x);
        // A whole number of points per sprite pixel.
        assert_eq!(cat.width() / 32.0, (cat.width() / 32.0).round());
    }

    #[test]
    fn top_half_moves_up_and_bottom_half_moves_down() {
        let directions: Vec<f32> = (0..RAINBOW_BANDS).map(displacement_direction).collect();
        // Negative `y` is up on screen.
        assert_eq!(
            directions,
            vec![-1.0, -(2.0 / 3.0), -(1.0 / 3.0), 1.0 / 3.0, 2.0 / 3.0, 1.0]
        );
    }

    #[test]
    fn rainbow_bands_are_averaged() {
        let bands = [1.0, 1.0, 0.0, 0.0, 0.5, 0.5, 0.2, 0.2, 0.0, 0.0, 1.0, 1.0];
        let rainbow = to_rainbow_bands(&bands);
        assert_eq!(rainbow, [1.0, 0.0, 0.5, 0.2, 0.0, 1.0]);
    }

    #[test]
    fn draw_with_egui() {
        let ctx = egui::Context::default();
        for speed in [0.0, 300.0] {
            let mut settings = VisualisationSettings {
                visualiser: VisualisationType::Nyan(NyanSettings::default()),
                window_size: Vec2::new(600.0, 300.0),
                ..VisualisationSettings::default()
            };
            settings.visualiser_settings.nyan.speed = speed;
            settings.visualiser_settings.nyan.window_width = if speed > 0.0 { 300.0 } else { 0.0 };
            for _ in 0..3 {
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    Nyan(NyanSettings::default()).draw(
                        ui,
                        &[0.1, 0.5, 0.9, 0.3, 0.7, 0.2, 0.4],
                        &settings,
                    );
                });
                output.textures_delta.clear();
            }
        }
    }
}
