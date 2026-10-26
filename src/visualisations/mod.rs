use egui::Vec2;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use strum::EnumIter;

pub mod equalizer;
pub mod nyan;
pub mod segmented_spectrum_bars;
pub mod waveform;

use equalizer::EqualizerSettings;
use nyan::NyanSettings;
use segmented_spectrum_bars::SegmentedSpectrumBars;
use waveform::WaveformSettings;

/// Upper bound for the number of analysed frequency bands.
pub const MAX_BANDS: usize = 512;

pub trait Visualisation {
    fn draw(&self, ui: &mut egui::Ui, bands: &[f32], settings: &VisualisationSettings);
    fn default_settings() -> VisualisationSettings;
}

#[derive(Debug, Copy, Clone, EnumIter, PartialEq, Serialize, Deserialize)]
pub enum VisualisationType {
    Equalizer,
    Waveform,
    Nyan,
    SegmentedSpectrumBars,
}

/// Settings shared by all visualisations, plus the individual settings of each visualisation.
/// This struct is what gets persisted to the configuration file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VisualisationSettings {
    pub visualiser: VisualisationType,
    pub gain: f32,
    pub smoothing: f32,
    pub window_size: Vec2,
    pub band_gap: f32,
    pub visualiser_settings: VisualiserSettings,
}

impl Default for VisualisationSettings {
    fn default() -> Self {
        Self {
            visualiser: VisualisationType::Equalizer,
            gain: 1.0,
            smoothing: 0.0,
            window_size: Vec2::new(800.0, 80.0),
            band_gap: 0.0,
            visualiser_settings: VisualiserSettings::default(),
        }
    }
}

/// Two settings are equal when all *configurable* values are equal.
///
/// The runtime-only `window_size` is deliberately ignored, so comparing a snapshot with the
/// current settings tells whether anything worth saving has changed.
impl PartialEq for VisualisationSettings {
    fn eq(&self, other: &Self) -> bool {
        self.visualiser == other.visualiser
            && self.gain == other.gain
            && self.smoothing == other.smoothing
            && self.band_gap == other.band_gap
            && self.visualiser_settings == other.visualiser_settings
    }
}

impl VisualisationSettings {
    /// Clamps every value into a range the visualisations and the analyser can handle.
    /// Used for values coming from the configuration file, the API and the GUI.
    pub fn sanitize(&mut self) {
        self.gain = self.gain.clamp(0.0, 100.0);
        self.smoothing = self.smoothing.clamp(0.0, 1.0);
        self.band_gap = self.band_gap.clamp(0.0, 256.0);
        self.visualiser_settings.sanitize();
    }

    pub fn bands(&self) -> usize {
        match self.visualiser {
            VisualisationType::Equalizer => self.visualiser_settings.equalizer.bands,
            VisualisationType::Waveform => self.visualiser_settings.waveform.bands,
            VisualisationType::Nyan => 6,
            VisualisationType::SegmentedSpectrumBars => {
                self.visualiser_settings.segmented_spectrum_bars.bands
            }
        }
    }
}

/// The individual settings of every visualisation that has any.
/// Every visualisation keeps its own section, so switching between visualisations does not
/// lose the configuration of the others.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VisualiserSettings {
    pub equalizer: EqualizerSettings,
    pub waveform: WaveformSettings,
    pub nyan: NyanSettings,
    pub segmented_spectrum_bars: SegmentedSpectrumBars,
}

/// A partial update of [`VisualiserSettings`].
/// Each section is a JSON object whose keys replace the corresponding fields of that section;
/// fields that are not mentioned keep their value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualiserSettingsPatch {
    pub equalizer: Option<Map<String, Value>>,
    pub waveform: Option<Map<String, Value>>,
    pub nyan: Option<Map<String, Value>>,
    pub segmented_spectrum_bars: Option<Map<String, Value>>,
}

impl VisualiserSettings {
    pub fn apply_patch(&mut self, patch: &VisualiserSettingsPatch) -> Result<(), String> {
        let mut updated = self.clone();
        merge_section(
            "equalizer",
            &mut updated.equalizer,
            patch.equalizer.as_ref(),
        )?;
        merge_section("waveform", &mut updated.waveform, patch.waveform.as_ref())?;
        merge_section("nyan", &mut updated.nyan, patch.nyan.as_ref())?;
        merge_section(
            "segmented_spectrum_bars",
            &mut updated.segmented_spectrum_bars,
            patch.segmented_spectrum_bars.as_ref(),
        )?;
        updated.sanitize();
        *self = updated;
        Ok(())
    }

    pub fn sanitize(&mut self) {
        self.nyan.sanitize();
        self.segmented_spectrum_bars.sanitize();
        self.equalizer.sanitize();
        self.waveform.sanitize();
    }
}

fn merge_section<T: Serialize + DeserializeOwned>(
    name: &str,
    target: &mut T,
    patch: Option<&Map<String, Value>>,
) -> Result<(), String> {
    let Some(patch) = patch else {
        return Ok(());
    };

    let Value::Object(mut current) = serde_json::to_value(&*target)
        .map_err(|e| format!("Failed to read current '{name}' settings: {e}"))?
    else {
        return Err(format!("Settings section '{name}' is not an object"));
    };

    for (key, value) in patch {
        if !current.contains_key(key) {
            return Err(format!("Unknown setting '{key}' in '{name}'"));
        }
        current.insert(key.clone(), value.clone());
    }

    *target = serde_json::from_value(Value::Object(current))
        .map_err(|e| format!("Invalid value in '{name}' settings: {e}"))?;
    Ok(())
}

impl VisualisationType {
    /// Draws the editor for the individual settings of this visualisation.
    /// Returns `true` if any value was changed.
    pub fn settings_ui(&self, ui: &mut egui::Ui, settings: &mut VisualiserSettings) -> bool {
        match self {
            Self::Equalizer => settings.equalizer.settings_ui(ui),
            Self::Waveform => settings.waveform.settings_ui(ui),
            Self::Nyan => settings.nyan.settings_ui(ui),
            Self::SegmentedSpectrumBars => settings.segmented_spectrum_bars.settings_ui(ui),
        }
    }
}

impl Visualisation for VisualisationType {
    fn draw(&self, ui: &mut egui::Ui, bands: &[f32], settings: &VisualisationSettings) {
        match self {
            Self::Equalizer => equalizer::Equalizer.draw(ui, bands, settings),
            Self::Waveform => waveform::Waveform.draw(ui, bands, settings),
            Self::Nyan => nyan::Nyan.draw(ui, bands, settings),
            Self::SegmentedSpectrumBars => settings
                .visualiser_settings
                .segmented_spectrum_bars
                .draw(ui, bands, settings),
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
            VisualisationType::SegmentedSpectrumBars => write!(f, "Segmented Spectrum Bars"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Color32;
    use segmented_spectrum_bars::ColorMapping;

    fn patch(json: &str) -> VisualiserSettingsPatch {
        serde_json::from_str(json).expect("patch should deserialize")
    }

    #[test]
    fn settings_round_trip_through_json() {
        let mut settings = VisualisationSettings::default();
        settings.visualiser = VisualisationType::SegmentedSpectrumBars;
        settings
            .visualiser_settings
            .segmented_spectrum_bars
            .segment_count = 20;
        settings
            .visualiser_settings
            .segmented_spectrum_bars
            .color_mapping = ColorMapping::Solid(Color32::from_rgb(1, 2, 3));
        settings.visualiser_settings.waveform.color = Color32::from_rgb(9, 8, 7);

        let json = serde_json::to_string(&settings).unwrap();
        let restored: VisualisationSettings = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.visualiser, settings.visualiser);
        assert_eq!(restored.visualiser_settings, settings.visualiser_settings);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let restored: VisualisationSettings =
            serde_json::from_str(r#"{"bands":24,"visualiser_settings":{"equalizer":{}}}"#).unwrap();

        assert_eq!(restored.gain, VisualisationSettings::default().gain);
        assert_eq!(
            restored.window_size,
            VisualisationSettings::default().window_size
        );
        assert_eq!(restored.visualiser_settings, VisualiserSettings::default());
    }

    #[test]
    fn patch_only_changes_mentioned_fields() {
        let mut settings = VisualiserSettings::default();
        settings
            .apply_patch(&patch(
                r#"{"segmented_spectrum_bars":{"segment_count":12,"band_gap":4.0}}"#,
            ))
            .unwrap();

        let bars = &settings.segmented_spectrum_bars;
        assert_eq!(bars.segment_count, 12);
        assert_eq!(bars.band_gap, 4.0);
        assert_eq!(
            bars.vertical_gap,
            SegmentedSpectrumBars::default().vertical_gap
        );
        assert_eq!(settings.equalizer, EqualizerSettings::default());
    }

    #[test]
    fn patch_can_reset_optional_and_replace_enum_fields() {
        let mut settings = VisualiserSettings::default();
        settings
            .apply_patch(&patch(
                r#"{"segmented_spectrum_bars":{"band_gap":3.0,"color_mapping":{"Solid":[255,0,0,255]}}}"#,
            ))
            .unwrap();
        assert_eq!(
            settings.segmented_spectrum_bars.color_mapping,
            ColorMapping::Solid(Color32::from_rgb(255, 0, 0))
        );

        settings
            .apply_patch(&patch(
                r#"{"segmented_spectrum_bars":{"band_gap":0.0,"color_mapping":"Rainbow"}}"#,
            ))
            .unwrap();
        assert_eq!(settings.segmented_spectrum_bars.band_gap, 0.0);
        assert_eq!(
            settings.segmented_spectrum_bars.color_mapping,
            ColorMapping::Rainbow
        );
    }

    #[test]
    fn patch_with_unknown_setting_is_rejected_and_nothing_changes() {
        let mut settings = VisualiserSettings::default();
        let result = settings.apply_patch(&patch(
            r#"{"equalizer":{"corner_radius":5},"segmented_spectrum_bars":{"nope":1}}"#,
        ));

        assert!(result.unwrap_err().contains("nope"));
        assert_eq!(settings, VisualiserSettings::default());
    }

    #[test]
    fn patch_with_invalid_value_is_rejected() {
        let mut settings = VisualiserSettings::default();
        let result = settings.apply_patch(&patch(r#"{"equalizer":{"corner_radius":300}}"#));

        assert!(result.is_err());
        assert_eq!(settings, VisualiserSettings::default());
    }

    #[test]
    fn nyan_settings_can_be_patched() {
        let mut settings = VisualiserSettings::default();
        settings
            .apply_patch(&patch(r#"{"nyan":{"line_gap":6.0,"speed":50.0}}"#))
            .unwrap();

        assert_eq!(settings.nyan.line_gap, 6.0);
        assert_eq!(settings.nyan.speed, 50.0);
        assert_eq!(settings.nyan.line_width, NyanSettings::default().line_width);

        // Out-of-range values are clamped.
        settings
            .apply_patch(&patch(r#"{"nyan":{"line_width":0.0,"speed":-5.0}}"#))
            .unwrap();
        assert_eq!(settings.nyan.line_width, 1.0);
        assert_eq!(settings.nyan.speed, 0.0);
    }

    #[test]
    fn nyan_window_size_can_be_patched() {
        let mut settings = VisualiserSettings::default();
        assert_eq!(settings.nyan.window_width, 1600.0);
        assert_eq!(settings.nyan.window_height, 200.0);

        settings
            .apply_patch(&patch(
                r#"{"nyan":{"window_width":640.0,"window_height":360.0}}"#,
            ))
            .unwrap();
        assert_eq!(settings.nyan.window_width, 640.0);
        assert_eq!(settings.nyan.window_height, 360.0);

        // Negative sizes are clamped to "fit the window".
        settings
            .apply_patch(&patch(r#"{"nyan":{"window_width":-1.0}}"#))
            .unwrap();
        assert_eq!(settings.nyan.window_width, 0.0);
        assert_eq!(settings.nyan.window_height, 360.0);
    }

    #[test]
    fn unknown_section_is_rejected_when_parsing_the_patch() {
        assert!(serde_json::from_str::<VisualiserSettingsPatch>(r#"{"unknown":{}}"#).is_err());
    }

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        let mut settings = VisualisationSettings {
            gain: -1.0,
            smoothing: 7.0,
            band_gap: -3.0,
            ..VisualisationSettings::default()
        };
        settings
            .visualiser_settings
            .segmented_spectrum_bars
            .segment_count = 0;
        settings.sanitize();

        assert_eq!(settings.gain, 0.0);
        assert_eq!(settings.smoothing, 1.0);
        assert_eq!(settings.band_gap, 0.0);
        assert_eq!(
            settings
                .visualiser_settings
                .segmented_spectrum_bars
                .segment_count,
            1
        );

        settings.sanitize();
    }

    #[test]
    fn sanitize_sorts_and_clamps_gradient_stops() {
        let mut settings = VisualiserSettings::default();
        settings.segmented_spectrum_bars.color_mapping = ColorMapping::Gradient(vec![
            (2.0, Color32::WHITE),
            (-1.0, Color32::BLACK),
            (0.5, Color32::RED),
        ]);
        settings.sanitize();

        let ColorMapping::Gradient(stops) = &settings.segmented_spectrum_bars.color_mapping else {
            panic!("expected a gradient");
        };
        let positions: Vec<f32> = stops.iter().map(|s| s.0).collect();
        assert_eq!(positions, vec![0.0, 0.5, 1.0]);
    }
}
