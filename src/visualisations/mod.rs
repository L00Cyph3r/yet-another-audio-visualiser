use egui::Vec2;
use serde::{Deserialize, Serialize};
use strum::EnumIter;

pub mod equalizer;
pub mod nyan;
pub mod waveform;

pub trait Visualisation {
    fn draw(&self, ui: &mut egui::Ui, bands: &[f32], settings: &VisualisationSettings);
    fn default_settings() -> VisualisationSettings;
}

#[derive(Debug, Copy, Clone, EnumIter, PartialEq, Serialize, Deserialize)]
pub enum VisualisationType {
    Equalizer,
    Waveform,
    Nyan,
}

pub struct VisualisationSettings {
    pub visualiser: VisualisationType,
    pub bands: usize,
    pub gain: f32,
    pub smoothing: f32,
    pub window_size: Vec2,
    pub band_gap: f32,
}

impl Default for VisualisationSettings {
    fn default() -> Self {
        Self {
            visualiser: VisualisationType::Equalizer,
            bands: 10,
            gain: 0.0,
            smoothing: 0.0,
            window_size: Vec2::new(1024.0, 100.0),
            band_gap: 0.0,
        }
    }
}

impl Visualisation for VisualisationType {
    fn draw(&self, ui: &mut egui::Ui, bands: &[f32], settings: &VisualisationSettings) {
        match self {
            Self::Equalizer => equalizer::Equalizer.draw(ui, bands, settings),
            Self::Waveform => waveform::Waveform.draw(ui, bands, settings),
            Self::Nyan => nyan::Nyan.draw(ui, bands, settings),
        }
    }

    fn default_settings() -> VisualisationSettings {
        VisualisationSettings::default()
    }
}

impl Default for VisualisationType {
    fn default() -> Self {
        VisualisationType::Equalizer
    }
}

impl std::fmt::Display for VisualisationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VisualisationType::Equalizer => write!(f, "Equalizer"),
            VisualisationType::Waveform => write!(f, "Waveform"),
            VisualisationType::Nyan => write!(f, "Nyan"),
        }
    }
}
