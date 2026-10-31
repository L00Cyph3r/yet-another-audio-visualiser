//! Benchmarks for the per-frame cost of each visualisation's `draw`.
//!
//! Run with `cargo bench --bench visualisations`.
//!
//! Every visualisation is driven through the public `Visualisation` trait, exactly the way
//! `GuiApp` drives it, so the numbers include the settings clone that the dispatcher performs.
//! Drawing goes into an offscreen `egui::Context`, so no window or GPU is needed.

use divan::Bencher;
use egui::{Pos2, Rect, Vec2};
use std::time::Duration;
use yet_another_audio_visualiser::visualisations::segmented_spectrum_bars::{
    ColorMapping, SegmentedSpectrumBarsSettings,
};
use yet_another_audio_visualiser::visualisations::{
    Visualisation, VisualisationSettings, VisualisationType,
};

fn main() {
    divan::main();
}

/// The window the visualisation is drawn into, in points. Matches the app's default viewport.
fn screen_rect() -> Rect {
    Rect::from_min_size(Pos2::ZERO, Vec2::new(1024.0, 512.0))
}

/// A spectrum with a plausible spread of levels across `bands` frequency bands.
fn levels(bands: usize) -> Vec<f32> {
    (0..bands)
        .map(|i| {
            let t = i as f32 / bands.max(1) as f32;
            // Falling towards the high end, with some structure in between.
            (0.9 * (1.0 - t) + 0.1 * (t * 40.0).sin().abs()).clamp(0.0, 1.0)
        })
        .collect()
}

/// Draws one frame of `settings.visualiser` into a fresh offscreen context.
///
/// A new `Context` is created per call so that no state carried in egui's temporary data (such as
/// the Nyan trail) can leak between iterations, and so the measurement is independent of the
/// iteration count.
fn draw_frame(settings: &VisualisationSettings, bands: &[f32]) {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(screen_rect()),
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| {
        settings.visualiser.draw(ui, bands, settings);
    });
    // `FullOutput` debug-asserts when dropped with unapplied texture deltas.
    output.drop_without_applying_deltas();
}

fn settings_for(visualiser: VisualisationType, bands: usize) -> VisualisationSettings {
    let mut settings = VisualisationSettings {
        visualiser,
        window_size: Vec2::new(1024.0, 512.0),
        ..VisualisationSettings::default()
    };
    // The analyser produces `bands` levels; the visualisations clamp this themselves, but the
    // input slice has to match what the app would pass in.
    let _ = bands;
    settings.sanitize();
    settings
}

#[divan::bench(min_time = Duration::from_secs(1))]
fn equalizer(bencher: Bencher) {
    let settings = settings_for(VisualisationType::default(), 48);
    let bands = levels(48);

    bencher.bench_local(|| draw_frame(&settings, &bands));
}

#[divan::bench(min_time = Duration::from_secs(1))]
fn waveform(bencher: Bencher) {
    let settings = settings_for(VisualisationType::Waveform(Default::default()), 48);
    let bands = levels(48);

    bencher.bench_local(|| draw_frame(&settings, &bands));
}

#[divan::bench(min_time = Duration::from_secs(1))]
fn nyan(bencher: Bencher) {
    let settings = settings_for(VisualisationType::Nyan(Default::default()), 6);
    let bands = levels(6);

    bencher.bench_local(|| draw_frame(&settings, &bands));
}

/// The most expensive configuration: the maximum band count, the maximum segment count and
/// inactive segments drawn as well, so every bar emits its full stack of rectangles.
#[divan::bench(min_time = Duration::from_secs(1), args = [48, 128, 512])]
fn segmented_spectrum_bars(bencher: Bencher, bands: usize) {
    let visualiser_settings = SegmentedSpectrumBarsSettings {
        bands,
        segment_count: 256,
        draw_inactive_segments: true,
        color_mapping: ColorMapping::Rainbow,
        ..SegmentedSpectrumBarsSettings::default()
    };

    let settings = settings_for(
        VisualisationType::SegmentedSpectrumBars(visualiser_settings),
        bands,
    );
    let bands = levels(bands);

    bencher.bench_local(|| draw_frame(&settings, &bands));
}
