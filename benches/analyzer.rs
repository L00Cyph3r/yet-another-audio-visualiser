//! Benchmarks for the spectrum analysis pipeline.
//!
//! Run with `cargo bench --bench analyzer`.
//!
//! These cover the per-frame work that [`GuiApp`](yet_another_audio_visualiser::gui::app::GuiApp)
//! performs on the audio path: draining the capture channel, and the windowing, FFT and band
//! grouping inside `SpectrumAnalyzer::update`.

use divan::Bencher;
use std::time::Duration;
use yet_another_audio_visualiser::analyzer::{
    DEFAULT_FFT_SIZE, SpectrumAnalyzer, SpectrumAnalyzerSettings,
};

fn main() {
    divan::main();
}

/// A deterministic, non-trivial signal: a few sine components plus a little noise-like wobble.
/// Deterministic so that runs are comparable, and non-silent so that the band loop does real work.
fn signal(len: usize, sample_rate: u32) -> Vec<f32> {
    (0..len)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            let mut sample = 0.0;
            for (index, frequency) in [110.0_f32, 440.0, 1760.0, 6000.0].iter().enumerate() {
                let amplitude = 0.25 / (index + 1) as f32;
                sample += amplitude * (2.0 * std::f32::consts::PI * frequency * t).sin();
            }
            sample
        })
        .collect()
}

/// A full buffer, so that `update` runs the windowing, FFT and band grouping rather than
/// returning early.
fn primed_analyzer(settings: SpectrumAnalyzerSettings, samples: &[f32]) -> SpectrumAnalyzer {
    let mut analyzer = SpectrumAnalyzer::new(settings);
    for &sample in samples {
        analyzer.push_sample(sample);
    }
    analyzer
}

/// How many samples one 60 Hz frame has to consume at 44.1 kHz.
#[divan::bench(min_time = Duration::from_secs(1))]
fn samples_per_frame(bencher: Bencher) {
    let sample_rate = 44_100;
    let samples = signal(DEFAULT_FFT_SIZE, sample_rate);
    let per_frame = sample_rate / 60;

    bencher.counter(per_frame).bench_local(|| {
        let mut analyzer = primed_analyzer(SpectrumAnalyzerSettings::default(), &samples);
        for &sample in samples.iter().take(per_frame as usize) {
            analyzer.push_sample(sample);
        }
    });
}

/// The whole per-frame analysis, for the default and the maximum band counts.
#[divan::bench(min_time = Duration::from_secs(1), args = [8, 64, 128, 512])]
fn update(bencher: Bencher, bands: usize) {
    let samples = signal(DEFAULT_FFT_SIZE, 44_100);

    bencher
        .counter(DEFAULT_FFT_SIZE)
        .with_inputs(|| {
            primed_analyzer(
                SpectrumAnalyzerSettings {
                    bands,
                    ..SpectrumAnalyzerSettings::default()
                },
                &samples,
            )
        })
        .bench_local_refs(|analyzer| analyzer.update());
}

/// Isolates the Hann window from the FFT: `update` always does both, so measuring it on its own
/// says how much of the total is spent recomputing the window.
#[divan::bench(min_time = Duration::from_secs(1), args = [1024,2048,4096,8192,16384,32768,65536])]
fn hann_window(bencher: Bencher, fft_size: usize) {
    let samples = signal(fft_size, 44_100);
    let mut buffer = vec![0.0_f32; fft_size];

    bencher.counter(fft_size).bench_local(|| {
        for (i, (&sample, out)) in samples.iter().zip(buffer.iter_mut()).enumerate() {
            let window =
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (fft_size - 1) as f32).cos());
            *out = sample * window;
        }
    });
}
