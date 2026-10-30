use crate::visualisations::VisualisationSettings;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "yet-another-audio-visualiser";
const FILE_NAME: &str = "settings.json";

/// Everything that is persisted in the configuration file.
/// Fields missing from the file fall back to their defaults, so settings added in newer
/// versions of the application do not invalidate an existing configuration file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub device: Option<String>,
    pub visualisation: VisualisationSettings,
}

impl AppSettings {
    pub fn sanitize(&mut self) {
        self.visualisation.sanitize();
    }
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The store location in the platform's configuration directory,
    /// e.g. `%APPDATA%\yet-another-audio-visualiser\settings.json` on Windows.
    pub fn default_location() -> Option<Self> {
        dirs::config_dir().map(|dir| Self::new(dir.join(APP_DIR_NAME).join(FILE_NAME)))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads the settings. This never fails: a missing or unreadable file yields the defaults,
    /// and an invalid file is moved aside (`settings.json.corrupt`) so that it is not silently
    /// overwritten by the next save.
    pub fn load(&self) -> AppSettings {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                debug!(
                    "No settings file at {}, using defaults",
                    self.path.display()
                );
                return AppSettings::default();
            }
            Err(e) => {
                warn!(
                    "Could not read {}: {e}; using defaults",
                    self.path.display()
                );
                return AppSettings::default();
            }
        };

        match serde_json::from_str::<AppSettings>(&text) {
            Ok(mut settings) => {
                settings.sanitize();
                info!("Loaded settings from {}", self.path.display());
                settings
            }
            Err(e) => {
                let backup = self.path.with_extension("json.corrupt");
                match fs::rename(&self.path, &backup) {
                    Ok(()) => warn!(
                        "Invalid settings file ({e}); moved it to {} and using defaults",
                        backup.display()
                    ),
                    Err(rename_err) => warn!(
                        "Invalid settings file ({e}) and it could not be moved aside \
                         ({rename_err}); using defaults"
                    ),
                }
                AppSettings::default()
            }
        }
    }

    /// Saves the settings atomically: the data is written to a temporary file first and then
    /// renamed over the real one, so a crash cannot leave a half-written configuration behind.
    pub fn save(&self, settings: &AppSettings) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visualisations::VisualisationType;
    use crate::visualisations::waveform::WaveformSettings;
    use egui::Color32;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("yaav-test-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }

        fn store(&self) -> SettingsStore {
            SettingsStore::new(self.0.join("nested").join(FILE_NAME))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_file_yields_defaults() {
        let dir = TempDir::new("missing");
        assert_eq!(dir.store().load(), AppSettings::default());
    }

    #[test]
    fn save_creates_directories_and_round_trips() {
        let dir = TempDir::new("round-trip");
        let store = dir.store();

        let mut settings = AppSettings::default();
        settings.device = Some("My Device".to_string());
        settings.visualisation.visualiser =
            VisualisationType::Waveform(WaveformSettings::default());
        settings.visualisation.visualiser_settings.waveform.color = Color32::from_rgb(1, 2, 3);
        settings
            .visualisation
            .visualiser_settings
            .segmented_spectrum_bars
            .segment_count = 21;

        store.save(&settings).unwrap();
        assert_eq!(store.load(), settings);
        assert!(!store.path().with_extension("json.tmp").exists());
    }

    #[test]
    fn save_overwrites_existing_file() {
        let dir = TempDir::new("overwrite");
        let store = dir.store();

        let settings = AppSettings::default();
        store.save(&settings).unwrap();
        store.save(&settings).unwrap();
    }

    #[test]
    fn invalid_file_is_moved_aside_and_defaults_are_used() {
        let dir = TempDir::new("corrupt");
        let store = dir.store();
        fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        fs::write(store.path(), "{ this is not json").unwrap();

        assert_eq!(store.load(), AppSettings::default());
        assert!(!store.path().exists());
        assert_eq!(
            fs::read_to_string(store.path().with_extension("json.corrupt")).unwrap(),
            "{ this is not json"
        );
    }

    #[test]
    fn values_are_sanitized_on_load() {
        let dir = TempDir::new("sanitize");
        let store = dir.store();
        fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        fs::write(
            store.path(),
            r#"{"visualisation":{"bands":0,"gain":-5.0,"visualiser_settings":{"segmented_spectrum_bars":{"segment_count":0}}}}"#,
        )
        .unwrap();

        let loaded = store.load();
        assert_eq!(loaded.visualisation.gain, 0.0);
        assert_eq!(
            loaded
                .visualisation
                .visualiser_settings
                .segmented_spectrum_bars
                .segment_count,
            1
        );
    }

    #[test]
    fn partial_file_keeps_defaults_for_missing_fields() {
        let dir = TempDir::new("partial");
        let store = dir.store();
        fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        fs::write(store.path(), r#"{"device":"Mic"}"#).unwrap();

        let loaded = store.load();
        assert_eq!(loaded.device.as_deref(), Some("Mic"));
        assert_eq!(loaded.visualisation, VisualisationSettings::default());
    }
}
