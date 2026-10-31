use egui::Vec2;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use strum::EnumIter;

use crate::profiling::{profile_function, profile_scope};

pub mod equalizer;
pub mod nyan;
pub mod segmented_spectrum_bars;
pub mod waveform;

use equalizer::{Equalizer, EqualizerSettings};
use nyan::{Nyan, NyanSettings};
use segmented_spectrum_bars::{SegmentedSpectrumBars, SegmentedSpectrumBarsSettings};
use waveform::{Waveform, WaveformSettings};

/// Upper bound for the number of analysed frequency bands.
pub const MAX_BANDS: usize = 512;

pub trait Visualisation {
    fn draw(&self, ui: &mut egui::Ui, bands: &[f32], settings: &VisualisationSettings);
    fn default_settings() -> VisualisationSettings;
}

#[derive(Debug, Clone, EnumIter, PartialEq)]
pub enum VisualisationType {
    Equalizer(EqualizerSettings),
    Waveform(WaveformSettings),
    Nyan(NyanSettings),
    SegmentedSpectrumBars(SegmentedSpectrumBarsSettings),
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
            visualiser: VisualisationType::Equalizer(EqualizerSettings::default()),
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

    /// Writes the settings of the currently active visualisation (the enum variant carried in
    /// `visualiser`) into the matching section of `visualiser_settings`. The other sections are
    /// left untouched, so switching visualisations does not lose their configuration. This keeps
    /// the persisted copy in sync with the values the GUI edits, making the changes saveable.
    pub fn sync_visualiser_settings(&mut self) {
        match &self.visualiser {
            VisualisationType::Equalizer(settings) => {
                self.visualiser_settings.equalizer = *settings
            }
            VisualisationType::Waveform(settings) => self.visualiser_settings.waveform = *settings,
            VisualisationType::Nyan(settings) => self.visualiser_settings.nyan = *settings,
            VisualisationType::SegmentedSpectrumBars(settings) => {
                self.visualiser_settings.segmented_spectrum_bars = settings.clone()
            }
        }
    }

    /// Loads the stored settings for the currently active visualisation from `visualiser_settings`
    /// into the enum variant carried in `visualiser`. Used when the application starts and when the
    /// visualisation type is switched, so that persisted values take effect.
    pub fn load_visualiser_settings(&mut self) {
        self.visualiser = match self.visualiser {
            VisualisationType::Equalizer(_) => {
                VisualisationType::Equalizer(self.visualiser_settings.equalizer)
            }
            VisualisationType::Waveform(_) => {
                VisualisationType::Waveform(self.visualiser_settings.waveform)
            }
            VisualisationType::Nyan(_) => VisualisationType::Nyan(self.visualiser_settings.nyan),
            VisualisationType::SegmentedSpectrumBars(_) => {
                VisualisationType::SegmentedSpectrumBars(
                    self.visualiser_settings.segmented_spectrum_bars.clone(),
                )
            }
        };
    }

    pub fn bands(&self) -> usize {
        match self.visualiser {
            VisualisationType::Equalizer(settings) => settings.bands,
            VisualisationType::Waveform(settings) => settings.bands,
            VisualisationType::Nyan(_) => 6,
            VisualisationType::SegmentedSpectrumBars(ref settings) => settings.bands,
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
    pub segmented_spectrum_bars: SegmentedSpectrumBarsSettings,
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
    pub fn settings_ui(&mut self, ui: &mut egui::Ui) -> bool {
        match self {
            Self::Equalizer(set) => set.settings_ui(ui),
            Self::Waveform(set) => set.settings_ui(ui),
            Self::Nyan(set) => set.settings_ui(ui),
            Self::SegmentedSpectrumBars(set) => set.settings_ui(ui),
        }
    }
    pub fn sanitize(&mut self) {
        match self {
            VisualisationType::Equalizer(set) => set.sanitize(),
            VisualisationType::Waveform(set) => set.sanitize(),
            VisualisationType::Nyan(set) => set.sanitize(),
            VisualisationType::SegmentedSpectrumBars(set) => set.sanitize(),
        }
    }
}

impl Visualisation for VisualisationType {
    fn draw(&self, ui: &mut egui::Ui, bands: &[f32], settings: &VisualisationSettings) {
        profile_function!();
        match self {
            Self::Equalizer(s) => Equalizer(*s).draw(ui, bands, settings),
            Self::Waveform(s) => Waveform(*s).draw(ui, bands, settings),
            Self::Nyan(s) => Nyan(*s).draw(ui, bands, settings),
            Self::SegmentedSpectrumBars(s) => {
                profile_scope!("segmented_spectrum_bars_clone");
                SegmentedSpectrumBars(s.clone()).draw(ui, bands, settings)
            }
        }
    }

    fn default_settings() -> VisualisationSettings {
        VisualisationSettings::default()
    }
}

impl Default for VisualisationType {
    fn default() -> Self {
        VisualisationType::Equalizer(EqualizerSettings::default())
    }
}

impl std::fmt::Display for VisualisationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VisualisationType::Equalizer(_) => write!(f, "Equalizer"),
            VisualisationType::Waveform(_) => write!(f, "Waveform"),
            VisualisationType::Nyan(_) => write!(f, "Nyan"),
            VisualisationType::SegmentedSpectrumBars(_) => write!(f, "Segmented Spectrum Bars"),
        }
    }
}

/// Serialises a visualisation as its plain type name (e.g. `"Waveform"`). The individual
/// settings carried in the enum variant are intentionally not serialised: `VisualisationSettings`
/// keeps the persisted copy of every visualisation's settings separately in `visualiser_settings`.
impl Serialize for VisualisationType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Accepts both the plain type name (e.g. `"Waveform"`, using that type's defaults) and the
/// object form carrying explicit settings (`{"Waveform": {...}}`).
impl<'de> Deserialize<'de> for VisualisationType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: Value = Value::deserialize(deserializer)?;
        let parsed = match value {
            Value::String(name) => variant_from_name(&name),
            Value::Object(map) => {
                let mut entries: Vec<(String, Value)> = map.into_iter().collect();
                if entries.len() != 1 {
                    Err("visualisation must contain exactly one entry".to_string())
                } else {
                    let (name, settings) = entries.remove(0);
                    deserialize_variant(&name, settings)
                }
            }
            _ => Err("visualisation must be a type name or an object with one variant".to_string()),
        };
        parsed.map_err(serde::de::Error::custom)
    }
}

/// Looks up the given type name and returns a [`VisualisationType`] with that type's defaults.
fn variant_from_name(name: &str) -> Result<VisualisationType, String> {
    match name {
        "Equalizer" => Ok(VisualisationType::Equalizer(EqualizerSettings::default())),
        "Waveform" => Ok(VisualisationType::Waveform(WaveformSettings::default())),
        "Nyan" => Ok(VisualisationType::Nyan(NyanSettings::default())),
        "SegmentedSpectrumBars" | "Segmented Spectrum Bars" => Ok(
            VisualisationType::SegmentedSpectrumBars(SegmentedSpectrumBarsSettings::default()),
        ),
        _ => Err(format!("Unknown visualisation type '{name}'")),
    }
}

/// Parses the settings of the given variant name from a serialised value.
fn deserialize_variant(name: &str, settings: Value) -> Result<VisualisationType, String> {
    match name {
        "Equalizer" => serde_json::from_value(settings)
            .map(VisualisationType::Equalizer)
            .map_err(|e| format!("Invalid Equalizer settings: {e}")),
        "Waveform" => serde_json::from_value(settings)
            .map(VisualisationType::Waveform)
            .map_err(|e| format!("Invalid Waveform settings: {e}")),
        "Nyan" => serde_json::from_value(settings)
            .map(VisualisationType::Nyan)
            .map_err(|e| format!("Invalid Nyan settings: {e}")),
        "SegmentedSpectrumBars" => serde_json::from_value(settings)
            .map(VisualisationType::SegmentedSpectrumBars)
            .map_err(|e| format!("Invalid SegmentedSpectrumBars settings: {e}")),
        _ => Err(format!("Unknown visualisation type '{name}'")),
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
        settings.visualiser =
            VisualisationType::SegmentedSpectrumBars(SegmentedSpectrumBarsSettings::default());
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
            SegmentedSpectrumBarsSettings::default().vertical_gap
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

    #[test]
    fn sync_writes_the_active_tuple_section_into_visualiser_settings() {
        let mut settings = VisualisationSettings::default();
        settings.visualiser = VisualisationType::Waveform(waveform::WaveformSettings {
            corner_radius: 9,
            color: Color32::from_rgb(1, 2, 3),
            ..waveform::WaveformSettings::default()
        });

        settings.sync_visualiser_settings();

        assert_eq!(settings.visualiser_settings.waveform.corner_radius, 9);
        assert_eq!(
            settings.visualiser_settings.waveform.color,
            Color32::from_rgb(1, 2, 3)
        );
        // Unrelated sections stay untouched.
        assert_eq!(
            settings.visualiser_settings.equalizer,
            EqualizerSettings::default()
        );
    }

    #[test]
    fn sync_only_updates_the_section_of_the_active_visualisation() {
        let mut settings = VisualisationSettings::default();
        settings
            .visualiser_settings
            .segmented_spectrum_bars
            .segment_count = 50;

        settings.visualiser = VisualisationType::Equalizer(EqualizerSettings {
            corner_radius: 17,
            band_gap: 3.0,
            ..EqualizerSettings::default()
        });
        settings.sync_visualiser_settings();

        assert_eq!(settings.visualiser_settings.equalizer.corner_radius, 17);
        assert_eq!(settings.visualiser_settings.equalizer.band_gap, 3.0);
        // The stored segmented_spectrum_bars section is preserved (switching must not lose it).
        assert_eq!(
            settings
                .visualiser_settings
                .segmented_spectrum_bars
                .segment_count,
            50
        );
    }

    #[test]
    fn load_populates_the_active_tuple_from_visualiser_settings() {
        let mut settings = VisualisationSettings::default();
        settings.visualiser = VisualisationType::Nyan(nyan::NyanSettings::default());
        settings.visualiser_settings.nyan.speed = 777.0;
        settings.visualiser_settings.nyan.line_width = 41.0;

        settings.load_visualiser_settings();

        let VisualisationType::Nyan(nyan) = settings.visualiser else {
            panic!("expected nyan");
        };
        assert_eq!(nyan.speed, 777.0);
        assert_eq!(nyan.line_width, 41.0);
    }

    #[test]
    fn sync_followed_by_load_is_a_no_op_for_the_active_section() {
        let mut settings = VisualisationSettings::default();
        settings.visualiser = VisualisationType::SegmentedSpectrumBars(
            segmented_spectrum_bars::SegmentedSpectrumBarsSettings {
                segment_count: 7,
                corner_radius: 3,
                ..segmented_spectrum_bars::SegmentedSpectrumBarsSettings::default()
            },
        );
        settings
            .visualiser_settings
            .segmented_spectrum_bars
            .segment_count = 123;

        // Inside the active visualisation, the tuple wins: syncing then loading yields the
        // tuple's values.
        settings.sync_visualiser_settings();
        settings.load_visualiser_settings();
        assert_eq!(
            settings
                .visualiser_settings
                .segmented_spectrum_bars
                .segment_count,
            7
        );
    }
    #[test]
    fn visualisation_type_serialises_as_plain_name() {
        let value = serde_json::to_value(VisualisationType::Waveform(waveform::WaveformSettings {
            color: Color32::from_rgb(1, 2, 3),
            ..waveform::WaveformSettings::default()
        }))
        .unwrap();
        assert_eq!(value, serde_json::json!("Waveform"));
    }

    #[test]
    fn visualisation_type_deserialises_from_plain_name_and_object() {
        // Plain type name uses defaults.
        let from_name: VisualisationType = serde_json::from_str("\"Nyan\"").unwrap();
        assert_eq!(
            from_name,
            VisualisationType::Nyan(nyan::NyanSettings::default())
        );

        // Object form carries explicit settings.
        let json = r#"{"Equalizer":{"band_gap":4.0}}"#;
        let from_object: VisualisationType = serde_json::from_str(json).unwrap();
        assert_eq!(
            from_object,
            VisualisationType::Equalizer(EqualizerSettings {
                band_gap: 4.0,
                ..EqualizerSettings::default()
            })
        );

        // Unknown names are rejected.
        assert!(serde_json::from_str::<VisualisationType>("\"Bogus\"").is_err());
        // Empty / multi-entry objects are rejected.
        assert!(serde_json::from_str::<VisualisationType>("{}").is_err());
        assert!(
            serde_json::from_str::<VisualisationType>(r#"{"Waveform":{},"Equalizer":{}}"#).is_err()
        );
    }
}
