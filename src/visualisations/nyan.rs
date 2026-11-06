use std::collections::VecDeque;

use crate::profiling::{profile_function, profile_scope};
use crate::visualisations::{Visualisation, VisualisationSettings, VisualisationType};
use egui::ViewportCommand::InnerSize;
use egui::{
    Color32, ColorImage, Pos2, Rect, Stroke, TextureHandle, TextureOptions, Ui, Vec2,
};
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

/// Distance (in points) between two recorded spectrum snapshots in the trail.
const SAMPLE_SPACING: f64 = 4.0;

const MAX_AMPLITUDE_FACTOR: f32 = 100.0;
const MAX_LINE_WIDTH: f32 = 200.0;
const MAX_LINE_GAP: f32 = 100.0;
const MAX_SPEED: f32 = 2000.0;
const MAX_WINDOW_SIZE: f32 = 8192.0;
const MAX_IMAGE_SCALE: f32 = 20.0;
const MAX_MOVEMENT_AMPLITUDE: f32 = 500.0;
const MAX_MOVEMENT_SPEED: f32 = 10.0;

/// The classic rainbow, from the top line to the bottom line.
const RAINBOW: [Color32; RAINBOW_BANDS] = [
    Color32::from_rgb(255, 0, 0),
    Color32::from_rgb(255, 153, 0),
    Color32::from_rgb(255, 255, 0),
    Color32::from_rgb(51, 255, 0),
    Color32::from_rgb(0, 153, 255),
    Color32::from_rgb(102, 51, 255),
];

/// How the Nyan image moves around the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NyanMovement {
    /// No movement: the image stays put.
    #[default]
    None,
    /// Gentle up-and-down bobbing.
    Floating,
    /// A circular orbit around the centre.
    Circular,
    /// A faster up-and-down sine wave (livelier than [`NyanMovement::Floating`]).
    Wavy,
}

impl NyanMovement {
    const ALL: [NyanMovement; 4] = [
        NyanMovement::None,
        NyanMovement::Floating,
        NyanMovement::Circular,
        NyanMovement::Wavy,
    ];

    fn label(self) -> &'static str {
        match self {
            NyanMovement::None => "None",
            NyanMovement::Floating => "Floating",
            NyanMovement::Circular => "Circular",
            NyanMovement::Wavy => "Wavy",
        }
    }
}

/// Parameters shared by the movement modes. Kept as a separate struct so that the values
/// persist even while the active mode is switched between them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MovementSettings {
    /// Cycles per second of the base oscillation.
    pub speed: f32,
    /// Distance (in points) the image travels from its rest position.
    pub amplitude: f32,
}

impl Default for MovementSettings {
    fn default() -> Self {
        Self {
            speed: 0.5,
            amplitude: 20.0,
        }
    }
}

impl MovementSettings {
    fn sanitize(&mut self) {
        self.speed = self.speed.clamp(0.0, MAX_MOVEMENT_SPEED);
        self.amplitude = self.amplitude.clamp(0.0, MAX_MOVEMENT_AMPLITUDE);
    }
}

/// Individual settings of the [`Nyan`] visualisation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NyanSettings {
    pub amplitude_factor: f32,
    pub line_width: f32,
    pub line_gap: f32,
    pub speed: f32,
    pub window_width: f32,
    pub window_height: f32,
    /// Scale of the image, independent of the window size. `1.0` keeps the previous
    /// behaviour of fitting the sprite to the window height.
    pub image_scale: f32,
    /// The movement mode applied to the image.
    pub movement: NyanMovement,
    /// Parameters for the movement modes; persisted independently of the active mode.
    pub movement_settings: MovementSettings,
    /// An optional path to a replacement sprite. `None` uses the embedded Nyan Cat.
    pub image_path: Option<String>,
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
            image_scale: 1.0,
            movement: NyanMovement::None,
            movement_settings: MovementSettings::default(),
            image_path: None,
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
        self.image_scale = self.image_scale.clamp(0.05, MAX_IMAGE_SCALE);
        self.movement_settings.sanitize();
        // Normalise an empty/whitespace path back to `None`.
        if let Some(path) = &self.image_path {
            if path.trim().is_empty() {
                self.image_path = None;
            }
        }
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

                ui.label("Image scale");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.image_scale)
                            .range(0.05..=MAX_IMAGE_SCALE)
                            .speed(0.05),
                    )
                    .on_hover_text("Scale of the image, independent of window size")
                    .changed();
                ui.end_row();

                ui.label("Movement");
                let before = self.movement;
                egui::ComboBox::from_id_salt("nyan_movement")
                    .selected_text(self.movement.label())
                    .show_ui(ui, |ui| {
                        for mode in NyanMovement::ALL {
                            changed |= ui
                                .selectable_value(&mut self.movement, mode, mode.label())
                                .changed();
                        }
                    });
                if self.movement != before {
                    changed = true;
                }
                ui.end_row();

                if self.movement != NyanMovement::None {
                    ui.label("Movement speed");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.movement_settings.speed)
                                .range(0.0..=MAX_MOVEMENT_SPEED)
                                .speed(0.05)
                                .suffix(" Hz"),
                        )
                        .changed();
                    ui.end_row();

                    ui.label("Movement amount");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.movement_settings.amplitude)
                                .range(0.0..=MAX_MOVEMENT_AMPLITUDE)
                                .speed(1.0)
                                .suffix(" px"),
                        )
                        .changed();
                    ui.end_row();
                }

                ui.label("Image");
                changed |= self.image_picker_ui(ui);
                ui.end_row();
            });
        changed
    }

    /// Draws the image path editor: a text field plus a "Browse…" button (PNG filter) and a
    /// "Clear" button to fall back to the embedded sprite.
    fn image_picker_ui(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;

        let mut text = self.image_path.clone().unwrap_or_default();
        if ui
            .add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text("Embedded Nyan Cat")
                    .desired_width(200.0),
            )
            .changed()
        {
            self.image_path = if text.trim().is_empty() {
                None
            } else {
                Some(text)
            };
            changed = true;
        }

        if ui.button("Browse…").clicked() {
            if let Some(path) = pick_image_file() {
                self.image_path = Some(path);
                changed = true;
            }
        }

        if ui.button("Clear").clicked() {
            if self.image_path.is_some() {
                self.image_path = None;
                changed = true;
            }
        }

        changed
    }
}

/// Opens a native file dialog filtered to PNG and GIF images. Returns the chosen path, or
/// `None` if the dialog was cancelled.
fn pick_image_file() -> Option<String> {
    rfd::FileDialog::new()
        .add_filter("Images (PNG, GIF)", &["png", "gif"])
        .pick_file()
        .map(|path| path.to_string_lossy().into_owned())
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
        profile_function!();
        let (window, _) = ui.allocate_exact_size(settings.window_size, egui::Sense::hover());
        let own = &self.0;
        ui.ctx()
            .send_viewport_cmd(InnerSize(Vec2::new(own.window_width, own.window_height)));
        let rect = own.visualisation_area(window);

        if bands.is_empty() || rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }

        let painter = ui.painter_at(rect);

        // Background colour, shared across all visualisations.
        painter.rect_filled(rect, 0.0, settings.background_color);

        let sprite = sprite(ui.ctx(), own.image_path.as_deref());
        let t = ui.input(|input| input.time);
        let (texture, size) = match sprite.as_ref() {
            Some(sprite) => {
                let (texture, size) = current_frame(sprite, t);
                (Some(texture), size)
            }
            None => (None, [0, 0]),
        };

        // The drawable image area, derived from the decoded size when an image is present.
        let cat_rect = texture
            .as_ref()
            .map(|_| image_rect(rect, size, own.image_scale));

        // The vertical centre of the rainbow is always the window's vertical centre, so the
        // cat (also vertically centred) and the rainbow centreline line up.
        let (origin_x, center_y) = match cat_rect {
            Some(cat) => (cat.left() + RAINBOW_ORIGIN_X * cat.width(), rect.center().y),
            None => (rect.right(), rect.center().y),
        };

        // The rainbow stays put: its origin is fixed at the window centre. Movement is applied
        // only to the image below.
        let origin = Pos2::new(origin_x, center_y);

        // Nyan is recreated by the enum dispatcher each frame, so the history
        // lives in egui's temporary widget state instead of in `Nyan` itself.
        let trail_id = ui.scope_id().with("nyan-rainbow-trail");
        let dt = ui.input(|input| input.stable_dt).clamp(0.0, 0.1);
        let current = to_rainbow_bands(bands);
        let trail = ui.ctx().data_mut(|data| {
            profile_scope!("trail_advance_and_clone");
            let trail = data.get_temp_mut_or_default::<Trail>(trail_id);
            trail.advance(dt * own.speed, current, origin.x - rect.left());
            trail.clone()
        });

        // Six parallel, coloured lines, stacked around the vertical centre. Each line is
        // displaced by its part of the spectrum: the top half moves up and the bottom half
        // moves down, away from the centre.
        let pitch = own.line_width + own.line_gap;
        let amplitude = own.line_width * settings.gain * own.amplitude_factor;
        let top_line_y = origin.y - pitch * (RAINBOW_BANDS - 1) as f32 / 2.0;

        // Draw from the bottom up so the top lines end up in front.
        for band in (0..RAINBOW_BANDS).rev() {
            profile_scope!("rainbow_line");
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
                    to_point(origin.x - age, sample.levels[band])
                })
                .collect();
            points.push(to_point(origin.x, trail.current[band]));

            if points.len() > 1 {
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(own.line_width, RAINBOW[band]),
                ));
            }
        }

        if let (Some(texture), Some(cat_rect)) = (texture, cat_rect) {
            let (dx, dy) = movement_offset(ui, own.movement, &own.movement_settings);
            let moved = cat_rect.translate(egui::vec2(dx, dy));
            painter.image(
                texture.id(),
                moved,
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

/// Computes the `(dx, dy)` offset for the given movement mode at the current time.
fn movement_offset(ui: &Ui, mode: NyanMovement, params: &MovementSettings) -> (f32, f32) {
    let t = ui.input(|input| input.time) as f32;
    movement_offset_at(t, mode, params)
}

/// Computes the `(dx, dy)` offset for the given movement mode at time `t` (in seconds).
fn movement_offset_at(t: f32, mode: NyanMovement, params: &MovementSettings) -> (f32, f32) {
    let amp = params.amplitude;
    // `2 * PI * speed` turns the given Hz value into radians per second.
    let omega = std::f32::consts::TAU * params.speed;

    match mode {
        NyanMovement::None => (0.0, 0.0),
        NyanMovement::Floating => (0.0, amp * (t * omega).sin()),
        NyanMovement::Circular => (amp * (t * omega).cos(), amp * (t * omega).sin()),
        NyanMovement::Wavy => (0.0, amp * (t * 2.0 * omega).sin()),
    }
}

/// The direction (in screen coordinates, where `y` grows downwards) in which a line moves
/// when its level rises: the top half of the rainbow moves up (`-1`), the bottom half moves
/// down (`+1`).
fn displacement_direction(band: usize) -> f32 {
    const BANDS_HALF: usize = RAINBOW_BANDS / 2;

    if band < BANDS_HALF {
        -(((BANDS_HALF - band) as f32) / BANDS_HALF as f32)
    } else {
        ((band - BANDS_HALF + 1) as f32) / BANDS_HALF as f32
    }
}

/// Where to draw the cat: at the right edge, vertically centred, as large as fits, scaled by
/// [`NyanSettings::cat_scale`]. The size is snapped to a whole number of screen points per
/// sprite pixel, which keeps the pixel art crisp.
fn image_rect(area: Rect, sprite_size: [usize; 2], cat_scale: f32) -> Rect {
    let (width, height) = (sprite_size[0] as f32, sprite_size[1] as f32);
    let fit = (area.height() * 0.9 / height).min(area.width() * 0.5 / width) * cat_scale;
    let pixel = if fit >= 1.0 { fit.floor() } else { fit };
    let size = egui::vec2(width * pixel, height * pixel);
    let right = area.right() - area.width() * 0.03;
    Rect::from_min_size(
        Pos2::new(right - size.x, area.center().y - size.y / 2.0),
        size,
    )
}

/// A decoded sprite, before it is uploaded as GPU textures.
enum SpriteSource {
    /// A single static image.
    Static(ColorImage),
    /// A looping animation; all frames share `size` (the GIF logical screen size).
    Animated {
        frames: Vec<ColorImage>,
        /// Per-frame delays in seconds, indexed in parallel with `frames`.
        delays: Vec<f64>,
        /// The logical screen size in pixels, common to every frame.
        size: [usize; 2],
    },
}

/// A GPU-ready decoded sprite, cached in the egui context.
#[derive(Clone)]
enum Sprite {
    Static { texture: TextureHandle, size: [usize; 2] },
    Animated {
        /// One texture per frame, parallel to `delays`.
        textures: Vec<TextureHandle>,
        /// Per-frame delays in seconds.
        delays: Vec<f64>,
        /// The logical screen size in pixels.
        size: [usize; 2],
    },
}

/// Loads and caches a sprite (decoded and uploaded to the GPU) keyed by `path`. `None` uses the
/// embedded Nyan Cat. A failed custom image falls back to the embedded sprite.
fn sprite(ctx: &egui::Context, path: Option<&str>) -> Option<Sprite> {
    let cache_key = match path {
        Some(path) => egui::Id::unique(("nyan-sprite", path.to_owned())),
        None => egui::Id::unique(("nyan-sprite", "__embedded__")),
    };

    if let Some(cached) = ctx.data(|data| data.get_temp::<Option<Sprite>>(cache_key)) {
        return cached;
    }

    let source = match path {
        Some(path) => match load_sprite_source(path) {
            Some(source) => source,
            None => {
                log::warn!("Could not load custom image; falling back to the embedded one");
                return sprite(ctx, None);
            }
        },
        None => match embedded_sprite_image() {
            Some(image) => SpriteSource::Static(image),
            None => {
                log::error!("Could not load the embedded Nyan Cat sprite");
                return None;
            }
        },
    };

    let sprite = match source {
        SpriteSource::Static(image) => {
            let size = image.size;
            let texture = ctx.load_texture("nyan-cat", image, TextureOptions::NEAREST);
            Sprite::Static { texture, size }
        }
        SpriteSource::Animated {
            frames,
            delays,
            size,
        } => {
            let textures = frames
                .into_iter()
                .map(|image| ctx.load_texture("nyan-cat-frame", image, TextureOptions::NEAREST))
                .collect();
            Sprite::Animated {
                textures,
                delays,
                size,
            }
        }
    };

    ctx.data_mut(|data| data.insert_temp(cache_key, Some(sprite.clone())));
    Some(sprite)
}

/// Returns the texture to draw for the current time `t` (in seconds), together with the size to
/// use for geometry.
fn current_frame(sprite: &Sprite, t: f64) -> (TextureHandle, [usize; 2]) {
    match sprite {
        Sprite::Static { texture, size } => (texture.clone(), *size),
        Sprite::Animated {
            textures,
            delays,
            size,
        } => {
            let index = frame_index_at(t, delays).min(textures.len().saturating_sub(1));
            (textures[index].clone(), *size)
        }
    }
}

/// Selects the frame to show at time `t` (seconds), looping forever over `delays`.
fn frame_index_at(t: f64, delays: &[f64]) -> usize {
    if delays.is_empty() {
        return 0;
    }
    let total: f64 = delays.iter().sum();
    if total <= 0.0 {
        return 0;
    }
    let mut remaining = t.rem_euclid(total);
    for (index, &delay) in delays.iter().enumerate() {
        if remaining < delay {
            return index;
        }
        remaining -= delay;
    }
    delays.len() - 1
}

/// Decodes a custom image file into a [`SpriteSource`], dispatching on file extension so that
/// animated GIFs are handled separately from static formats (e.g. PNG).
fn load_sprite_source(path: &str) -> Option<SpriteSource> {
    let is_gif = std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"));

    if is_gif {
        load_animated(path)
    } else {
        let rgba = image::open(path).ok()?.to_rgba8();
        decode_sprite(&rgba).map(SpriteSource::Static)
    }
}

/// Decodes an animated GIF from disk into per-frame composited RGBA images plus delays.
fn load_animated(path: &str) -> Option<SpriteSource> {
    let bytes = std::fs::read(path).ok()?;
    load_animated_bytes(&bytes)
}

/// Decodes an animated GIF from an in-memory byte buffer. Split out so it can be unit-tested
/// without touching the filesystem.
fn load_animated_bytes(bytes: &[u8]) -> Option<SpriteSource> {
    use gif::DisposalMethod;

    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(bytes).ok()?;

    let width = decoder.width() as usize;
    let height = decoder.height() as usize;
    if width == 0 || height == 0 {
        return None;
    }

    // The compositing canvas: transparent to start, matching a transparent GIF background.
    let mut canvas = vec![0u8; width * height * 4];
    let mut frames = Vec::new();
    let mut delays = Vec::new();

    while let Some(frame) = decoder.read_next_frame().ok().flatten() {
        let left = frame.left as usize;
        let top = frame.top as usize;
        let frame_width = frame.width as usize;
        let frame_height = frame.height as usize;
        let buffer = frame.buffer.clone();
        let delay = normalise_delay(frame.delay);
        let dispose = frame.dispose;

        // Snapshot the canvas *before* this frame so `Previous` disposal can restore it.
        let previous = canvas.clone();

        // Draw the frame: overwrite only opaque pixels so transparent pixels keep the current
        // canvas (the "Keep" behaviour the GIF spec expects).
        composite_frame(
            &mut canvas,
            width,
            left,
            top,
            frame_width,
            frame_height,
            &buffer,
        );

        // Record the composited result and delay.
        delays.push(delay);
        frames.push(canvas_to_color_image(&canvas, width, height));

        // Apply the disposal method to mutate the canvas for the next frame.
        match dispose {
            DisposalMethod::Keep | DisposalMethod::Any => {}
            DisposalMethod::Background => {
                clear_region(&mut canvas, width, left, top, frame_width, frame_height);
            }
            DisposalMethod::Previous => canvas = previous,
        }
    }

    if frames.is_empty() {
        return None;
    }

    Some(SpriteSource::Animated {
        frames,
        delays,
        size: [width, height],
    })
}

/// Normalises a GIF frame delay (in 10 ms units) to seconds. A delay of `0` means "unspecified"
/// and falls back to 100 ms.
fn normalise_delay(delay_centis: u16) -> f64 {
    if delay_centis == 0 {
        0.1
    } else {
        f64::from(delay_centis) * 0.01
    }
}

/// Composites one GIF frame (RGBA, frame-sized) onto `canvas` at `(left, top)`. Transparent
/// pixels leave the canvas unchanged.
fn composite_frame(
    canvas: &mut [u8],
    canvas_width: usize,
    left: usize,
    top: usize,
    frame_width: usize,
    frame_height: usize,
    frame_buffer: &[u8],
) {
    for row in 0..frame_height {
        for col in 0..frame_width {
            let src = (row * frame_width + col) * 4;
            let alpha = frame_buffer[src + 3];
            if alpha == 0 {
                continue;
            }
            let dst_x = left + col;
            let dst_y = top + row;
            if dst_x >= canvas_width {
                continue;
            }
            let dst = (dst_y * canvas_width + dst_x) * 4;
            if dst + 3 >= canvas.len() {
                continue;
            }
            canvas[dst..dst + 4].copy_from_slice(&frame_buffer[src..src + 4]);
        }
    }
}

/// Clears a rectangular region of the canvas to transparent, used by [`DisposalMethod::Background`].
fn clear_region(
    canvas: &mut [u8],
    canvas_width: usize,
    left: usize,
    top: usize,
    width: usize,
    height: usize,
) {
    for row in 0..height {
        for col in 0..width {
            let x = left + col;
            let y = top + row;
            if x >= canvas_width {
                continue;
            }
            let idx = (y * canvas_width + x) * 4;
            if idx + 3 >= canvas.len() {
                continue;
            }
            canvas[idx..idx + 4].fill(0);
        }
    }
}

/// Wraps a byte RGBA buffer into an egui [`ColorImage`].
fn canvas_to_color_image(canvas: &[u8], width: usize, height: usize) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([width, height], canvas)
}

/// Decodes the embedded sprite, trimming the transparent border and reducing it to one pixel
/// per pixel of the pixel art (the PNG is a 35x magnified version of it).
fn embedded_sprite_image() -> Option<ColorImage> {
    let rgba = image::load_from_memory_with_format(CAT_PNG, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    decode_sprite(&rgba)
}

/// Trims the transparent border around the sprite and, when the image is a whole multiple of
/// [`SOURCE_PIXEL_SIZE`], downsamples it to a single point per pixel-art pixel.
fn decode_sprite(rgba: &image::RgbaImage) -> Option<ColorImage> {
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
    let cropped = image::imageops::crop_imm(rgba, min_x, min_y, width, height).to_image();

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
        let image = embedded_sprite_image().expect("sprite should decode");
        assert_eq!(image.size, [32, 20]);
        // The image is opaque in the middle and transparent in the corners.
        assert_eq!(image.pixels[10 * 32 + 16].a(), 255);
        assert_eq!(image.pixels[0].a(), 0);
    }

    #[test]
    fn frame_index_at_selects_first_frame_at_zero() {
        assert_eq!(frame_index_at(0.0, &[0.1, 0.2, 0.3]), 0);
    }

    #[test]
    fn frame_index_at_accumulates_delays() {
        // Delays chosen to be exactly representable in binary to avoid boundary jitter.
        let delays = [0.25, 0.25];
        assert_eq!(frame_index_at(0.0, &delays), 0);
        assert_eq!(frame_index_at(0.2, &delays), 0);
        assert_eq!(frame_index_at(0.3, &delays), 1);
        assert_eq!(frame_index_at(0.4, &delays), 1);
    }

    #[test]
    fn frame_index_at_wraps_around() {
        let delays = [0.25, 0.25];
        // Total is 0.5 s; time beyond wraps modulo the loop length.
        assert_eq!(frame_index_at(0.5, &delays), 0);
        assert_eq!(frame_index_at(0.75, &delays), 1);
        assert_eq!(frame_index_at(1.0, &delays), 0);
        assert_eq!(frame_index_at(1.3, &delays), 1);
    }

    #[test]
    fn frame_index_at_handles_empty_and_zero() {
        assert_eq!(frame_index_at(0.0, &[]), 0);
        assert_eq!(frame_index_at(1.0, &[]), 0);
        assert_eq!(frame_index_at(0.5, &[0.0, 0.0]), 0);
    }

    #[test]
    fn normalise_delay_maps_centiseconds_to_seconds() {
        assert_eq!(normalise_delay(0), 0.1);
        assert_eq!(normalise_delay(10), 0.1);
        assert_eq!(normalise_delay(100), 1.0);
        assert_eq!(normalise_delay(250), 2.5);
    }

    #[test]
    fn composite_overwrites_opaque_and_preserves_transparent() {
        let mut canvas = vec![0u8; 2 * 2 * 4];
        // Frame is 2x2 with one opaque red pixel and three transparent pixels.
        let mut frame = vec![0u8; 2 * 2 * 4];
        frame[0..4].copy_from_slice(&[255, 0, 0, 255]); // opaque red at (0,0)
        // remaining pixels remain alpha 0 (transparent)

        composite_frame(&mut canvas, 2, 0, 0, 2, 2, &frame);

        // (0,0) becomes red.
        assert_eq!(&canvas[0..4], &[255, 0, 0, 255]);
        // (1,0) stays transparent.
        assert_eq!(&canvas[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn clear_region_sets_region_to_transparent() {
        let mut canvas = vec![255u8; 2 * 2 * 4];
        clear_region(&mut canvas, 2, 0, 0, 1, 1);
        assert_eq!(&canvas[0..4], &[0, 0, 0, 0]);
        // The untouched pixel stays opaque.
        assert_eq!(&canvas[4..8], &[255, 255, 255, 255]);
    }

    #[test]
    fn animated_gif_decodes_to_logical_size() {
        use gif::{Encoder, Frame, Repeat};

        // A 2x1 GIF with two frames: red, then green, both fully opaque. Uses an indexed
        // (palette) encoding so no `color_quant` feature is required.
        let palette: &[u8] = &[255, 0, 0, 0, 255, 0]; // index 0 = red, index 1 = green
        let mut data = Vec::new();
        {
            let mut encoder = Encoder::new(&mut data, 2, 1, palette).unwrap();
            encoder.set_repeat(Repeat::Infinite).unwrap();

            let mut red = Frame::from_indexed_pixels(2, 1, vec![0, 0], None);
            red.delay = 10;
            red.dispose = gif::DisposalMethod::Keep;
            encoder.write_frame(&red).unwrap();

            let mut green = Frame::from_indexed_pixels(2, 1, vec![1, 1], None);
            green.delay = 20;
            green.dispose = gif::DisposalMethod::Keep;
            encoder.write_frame(&green).unwrap();
        }

        let sprite = load_animated_bytes(&data).expect("gif should decode");
        let SpriteSource::Animated {
            frames,
            delays,
            size,
        } = sprite
        else {
            panic!("expected animated sprite");
        };

        assert_eq!(size, [2, 1]);
        assert_eq!(frames.len(), 2);
        assert_eq!(delays, vec![0.1, 0.2]);
        assert_eq!(frames[0].size, [2, 1]);
        assert_eq!(frames[1].size, [2, 1]);
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
            image_scale: 0.0,
            movement: NyanMovement::None,
            movement_settings: MovementSettings {
                speed: 1.0e9,
                amplitude: 1.0e9,
            },
            image_path: None,
        };
        settings.sanitize();
        assert_eq!(settings.line_width, 1.0);
        assert_eq!(settings.line_gap, 0.0);
        assert_eq!(settings.speed, MAX_SPEED);
        assert_eq!(settings.window_width, 0.0);
        assert_eq!(settings.window_height, MAX_WINDOW_SIZE);
        assert_eq!(settings.image_scale, 0.05);
        assert_eq!(settings.movement_settings.speed, MAX_MOVEMENT_SPEED);
        assert_eq!(settings.movement_settings.amplitude, MAX_MOVEMENT_AMPLITUDE);
    }

    #[test]
    fn blank_image_path_is_normalised_to_none() {
        let mut settings = NyanSettings {
            image_path: Some("   ".to_string()),
            ..NyanSettings::default()
        };
        settings.sanitize();
        assert_eq!(settings.image_path, None);
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
        let cat = image_rect(area, [32, 20], 1.0);
        assert!(area.contains_rect(cat));
        assert!(cat.center().x > area.center().x);
        // A whole number of points per sprite pixel.
        assert_eq!(cat.width() / 32.0, (cat.width() / 32.0).round());
    }

    #[test]
    fn cat_is_vertically_centred_on_the_area() {
        let area = Rect::from_min_size(Pos2::ZERO, egui::vec2(1024.0, 512.0));
        let cat = image_rect(area, [32, 20], 1.0);
        assert_eq!(cat.center().y, area.center().y);
    }

    #[test]
    fn cat_scale_scales_the_sprite() {
        let area = Rect::from_min_size(Pos2::ZERO, egui::vec2(1024.0, 512.0));
        let base = image_rect(area, [32, 20], 1.0);
        let doubled = image_rect(area, [32, 20], 2.0);
        assert!(doubled.width() > base.width());
        assert!(doubled.height() > base.height());
    }

    #[test]
    fn movement_none_is_zero() {
        let params = MovementSettings {
            speed: 1.0,
            amplitude: 10.0,
        };
        assert_eq!(movement_offset_at(1.23, NyanMovement::None, &params), (0.0, 0.0));
    }

    #[test]
    fn movement_offsets_are_bounded_by_amplitude() {
        let params = MovementSettings {
            speed: 1.0,
            amplitude: 10.0,
        };
        // Sample a few times; every offset component must stay within +/- amplitude.
        for t in [0.0, 0.1, 0.25, 0.5, 1.0, 2.0] {
            let (dx, dy) = movement_offset_at(t, NyanMovement::Circular, &params);
            assert!(dx.abs() <= 10.0 + f32::EPSILON, "dx out of range: {dx}");
            assert!(dy.abs() <= 10.0 + f32::EPSILON, "dy out of range: {dy}");
        }
    }

    #[test]
    fn floating_and_wavy_are_purely_vertical() {
        let params = MovementSettings {
            speed: 1.0,
            amplitude: 10.0,
        };
        for t in [0.0, 0.1, 0.25, 0.5, 1.0, 2.0] {
            let (dx, _) = movement_offset_at(t, NyanMovement::Floating, &params);
            assert_eq!(dx, 0.0, "Floating must not move horizontally");
            let (dx, _) = movement_offset_at(t, NyanMovement::Wavy, &params);
            assert_eq!(dx, 0.0, "Wavy must not move horizontally");
        }
    }

    #[test]
    fn movement_settings_round_trip() {
        let params = MovementSettings {
            speed: 1.5,
            amplitude: 33.0,
        };
        let json = serde_json::to_string(&params).unwrap();
        let restored: MovementSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, params);
    }

    #[test]
    fn nyan_settings_round_trip_with_new_fields() {
        let settings = NyanSettings {
            image_scale: 2.5,
            movement: NyanMovement::Wavy,
            movement_settings: MovementSettings {
                speed: 0.7,
                amplitude: 40.0,
            },
            image_path: Some("/tmp/cat.png".to_string()),
            ..NyanSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let restored: NyanSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, settings);
    }

    #[test]
    fn movement_enum_round_trip() {
        for mode in NyanMovement::ALL {
            let json = serde_json::to_string(&mode).unwrap();
            let restored: NyanMovement = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, mode);
        }
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
            settings.visualiser_settings.nyan.movement = NyanMovement::Circular;
            settings.visualiser_settings.nyan.movement_settings.speed = 0.5;
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
