use rustfft::{FftPlanner, num_complex::Complex32};
use std::collections::VecDeque;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

use crate::profiling::{profile_function, profile_scope};

const MIN_FREQ: f32 = 20.0;
const MAX_FREQ: f32 = 20_000.0;
pub const DEFAULT_FFT_SIZE: usize = 1024;
const NUM_BANDS: usize = 48;

/// Default frequency resolution (bin width) in Hz, matching the historical default of
/// 44100 / 1024 ≈ 43 Hz.
pub const DEFAULT_FREQUENCY_RESOLUTION: f32 = 43.0;

/// Bounds for the FFT size derived from a frequency resolution. Powers of two keep `rustfft`
/// on its fastest code path; the range keeps the visualisation responsive and the FFT cheap.
const MIN_FFT_SIZE: usize = 256;
const MAX_FFT_SIZE: usize = 65536;

/// Converts a desired frequency resolution (bin width, in Hz) into an FFT size for the given
/// sample rate. The raw size is `sample_rate / resolution_hz`, rounded up to the next power of
/// two and clamped to `[MIN_FFT_SIZE, MAX_FFT_SIZE]`. A smaller resolution (finer bins) yields a
/// larger FFT, which is more accurate but less responsive; a larger resolution yields a smaller,
/// more responsive FFT.
pub fn fft_size_for_resolution(sample_rate: u32, resolution_hz: f32) -> usize {
    if !resolution_hz.is_finite() || resolution_hz <= 0.0 {
        return DEFAULT_FFT_SIZE;
    }
    let raw = sample_rate as f32 / resolution_hz;
    if !raw.is_finite() || raw <= 0.0 {
        return DEFAULT_FFT_SIZE;
    }
    let size = (raw.ceil() as usize).next_power_of_two();
    size.clamp(MIN_FFT_SIZE, MAX_FFT_SIZE)
}

pub struct SpectrumAnalyzerSettings {
    pub bands: usize,
    pub fft_size: usize,
    pub sample_rate: u32,
    pub smoothing: f32,
}

impl Default for SpectrumAnalyzerSettings {
    fn default() -> Self {
        Self {
            bands: NUM_BANDS,
            smoothing: 0.5,
            fft_size: DEFAULT_FFT_SIZE,
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
    smoothing: f32,
}
impl Debug for SpectrumAnalyzer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpectrumAnalyzer")
            .field("fft", &self.fft.len())
            .field("samples", &self.samples.len())
            .field("fft_buffer", &self.fft_buffer.len())
            .field("bands", &self.bands.len())
            .field("fft_size", &self.fft_size)
            .field("sample_rate", &self.sample_rate)
            .field("bands_size", &self.bands_size)
            .finish()
    }
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
            smoothing: settings.smoothing,
        }
    }

    pub fn set_smoothing(&mut self, smoothing: f32) {
        self.smoothing = smoothing;
    }

    pub fn push_sample(&mut self, sample: f32) {
        if self.samples.len() == self.fft_size {
            self.samples.pop_front();
        }

        self.samples.push_back(sample);
    }

    pub fn update(&mut self) {
        profile_function!();

        if self.samples.len() < self.fft_size {
            return;
        }

        // Copy samples into the FFT buffer and apply a Hann window.
        {
            profile_scope!("hann_window");
            for (i, &sample) in self.samples.iter().enumerate() {
                let window = 0.5
                    * (1.0
                        - (2.0 * std::f32::consts::PI * i as f32 / (self.fft_size - 1) as f32)
                            .cos());

                self.fft_buffer[i] = Complex32::new(sample * window, 0.0);
            }
        }

        {
            profile_scope!("fft");
            self.fft.process(&mut self.fft_buffer);
        }

        let bin_width = self.sample_rate as f32 / self.fft_size as f32;

        // Create logarithmically spaced frequency bands.
        profile_scope!("bands");
        for band_index in 0..self.bands_size {
            let t0 = band_index as f32 / self.bands_size as f32;
            let t1 = (band_index + 1) as f32 / self.bands_size as f32;

            let min_frequency = MIN_FREQ * (MAX_FREQ / MIN_FREQ).powf(t0);
            let max_frequency = MIN_FREQ * (MAX_FREQ / MIN_FREQ).powf(t1);

            let start_bin = (min_frequency / bin_width).floor() as usize;
            let end_bin = (max_frequency / bin_width).ceil() as usize;

            let mut magnitude_sum = 0.0;
            let mut count = 0;

            for bin in start_bin..=end_bin.min(self.fft_size / 2 - 1) {
                magnitude_sum += self.fft_buffer[bin].norm();
                count += 1;
            }

            let magnitude = if count > 0 {
                magnitude_sum / count as f32
            } else {
                0.0
            };

            // Normalise for the FFT size.
            let normalized = magnitude / self.fft_size as f32;

            // Convert to decibels.
            let db = 20.0 * normalized.max(1e-7).log10();

            // Map roughly -80 dB..0 dB onto 0.0..1.0.
            let target = ((db + 90.0) / 90.0).clamp(0.0, 1.0);

            // Smooth the display. Higher values respond faster.
            let smoothing = if target > self.bands[band_index] {
                (1.0 / 3.0) + (self.smoothing / 3.0) // attack
            } else {
                (1.0 / 3.0) - (self.smoothing / 3.0) // release
            };

            self.bands[band_index] += (target - self.bands[band_index]) * smoothing;
        }
    }

    pub fn bands(&self) -> &[f32] {
        &self.bands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_maps_to_expected_fft_size() {
        // 44100 / 43 ≈ 1025.6 -> next power of two is 2048.
        assert_eq!(fft_size_for_resolution(44100, 43.0), 2048);
        // Exact power-of-two ratio stays put: 48000 / 46.875 = 1024.
        assert_eq!(fft_size_for_resolution(48000, 46.875), 1024);
        // Coarser resolution -> smaller FFT.
        assert_eq!(fft_size_for_resolution(44100, 44100.0 / 512.0), 512);
    }

    #[test]
    fn fft_size_is_always_a_power_of_two() {
        for resolution in [0.5_f32, 1.0, 7.3, 43.0, 100.0, 250.0, 500.0] {
            let size = fft_size_for_resolution(44100, resolution);
            assert!(size.is_power_of_two(), "{size} is not a power of two");
        }
    }

    #[test]
    fn fft_size_is_clamped_to_bounds() {
        // Extremely fine resolution would want a huge FFT; clamp to the max.
        assert_eq!(fft_size_for_resolution(44100, 0.0001), MAX_FFT_SIZE);
        // Extremely coarse resolution would want a tiny FFT; clamp to the min.
        assert_eq!(fft_size_for_resolution(44100, 1_000_000.0), MIN_FFT_SIZE);
    }

    #[test]
    fn invalid_resolution_falls_back_to_default() {
        assert_eq!(fft_size_for_resolution(44100, 0.0), DEFAULT_FFT_SIZE);
        assert_eq!(fft_size_for_resolution(44100, -5.0), DEFAULT_FFT_SIZE);
        assert_eq!(fft_size_for_resolution(44100, f32::NAN), DEFAULT_FFT_SIZE);
        assert_eq!(
            fft_size_for_resolution(44100, f32::INFINITY),
            DEFAULT_FFT_SIZE
        );
    }
}
