use rustfft::{FftPlanner, num_complex::Complex32};
use std::collections::VecDeque;
use std::sync::Arc;

const FFT_SIZE: usize = 2048;
const NUM_BANDS: usize = 48;

pub struct SpectrumAnalyzerSettings {
    pub bands: usize,
    pub gain: f32,
    pub fft_size: usize,
    pub sample_rate: u32,
}

impl Default for SpectrumAnalyzerSettings {
    fn default() -> Self {
        Self {
            bands: NUM_BANDS,
            gain: 1.0,
            fft_size: FFT_SIZE,
            sample_rate: 44100,
        }
    }
}

pub struct SpectrumAnalyzer {
    fft: Arc<dyn rustfft::Fft<f32>>,
    samples: VecDeque<f32>,
    fft_buffer: Vec<Complex32>,
    bands: Vec<f32>,
    sample_rate: u32,
    fft_size: usize,
    bands_size: usize,
}

impl SpectrumAnalyzer {
    pub fn new(settings: SpectrumAnalyzerSettings) -> Self {
        let mut planner = FftPlanner::<f32>::new();

        Self {
            fft: planner.plan_fft_forward(settings.fft_size),
            samples: VecDeque::with_capacity(settings.fft_size),
            fft_buffer: vec![Complex32::new(0.0, 0.0); settings.fft_size],
            bands: vec![0.0; settings.bands],
            sample_rate: settings.sample_rate,
            fft_size: settings.fft_size,
            bands_size: settings.bands,
        }
    }

    pub fn push_sample(&mut self, sample: f32) {
        if self.samples.len() == FFT_SIZE {
            self.samples.pop_front();
        }

        self.samples.push_back(sample);
    }

    pub fn update(&mut self) {
        if self.samples.len() < FFT_SIZE {
            return;
        }

        // Copy samples into the FFT buffer and apply a Hann window.
        for (i, &sample) in self.samples.iter().enumerate() {
            let window =
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos());

            self.fft_buffer[i] = Complex32::new(sample * window, 0.0);
        }

        self.fft.process(&mut self.fft_buffer);

        let nyquist = self.sample_rate as f32 / 2.0;
        let bin_width = self.sample_rate as f32 / FFT_SIZE as f32;

        // Create logarithmically spaced frequency bands.
        for band_index in 0..NUM_BANDS {
            let t0 = band_index as f32 / NUM_BANDS as f32;
            let t1 = (band_index + 1) as f32 / NUM_BANDS as f32;

            let min_frequency = 20.0 * (nyquist / 20.0).powf(t0);
            let max_frequency = 20.0 * (nyquist / 20.0).powf(t1);

            let start_bin = (min_frequency / bin_width).floor() as usize;
            let end_bin = (max_frequency / bin_width).ceil() as usize;

            let mut magnitude_sum = 0.0;
            let mut count = 0;

            for bin in start_bin..=end_bin.min(FFT_SIZE / 2 - 1) {
                magnitude_sum += self.fft_buffer[bin].norm();
                count += 1;
            }

            let magnitude = if count > 0 {
                magnitude_sum / count as f32
            } else {
                0.0
            };

            // Normalize for the FFT size.
            let normalized = magnitude / FFT_SIZE as f32;

            // Convert to decibels.
            let db = 20.0 * (normalized.max(1e-7)).log10();

            // Map roughly -80 dB..0 dB onto 0.0..1.0.
            let target = ((db + 80.0) / 80.0).clamp(0.0, 1.0);

            // Smooth the display. Higher values respond faster.
            let smoothing = if target > self.bands[band_index] {
                0.35 // attack
            } else {
                0.08 // release
            };

            self.bands[band_index] += (target - self.bands[band_index]) * smoothing;
        }
    }

    pub fn bands(&self) -> &[f32] {
        &self.bands
    }
}
