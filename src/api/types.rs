use crate::visualisations::{VisualisationType, VisualiserSettings, VisualiserSettingsPatch};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum AppCommand {
    UpdateSettings(
        SettingsPatch,
        Option<tokio::sync::oneshot::Sender<Result<SettingsDto, String>>>,
    ),
    SelectDevice(
        String,
        Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
    ),
    GetSettings(tokio::sync::oneshot::Sender<SettingsDto>),
    GetDevices(tokio::sync::oneshot::Sender<Vec<DeviceDto>>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SettingsDto {
    pub visualiser: VisualisationType,
    pub gain: f32,
    pub smoothing: f32,
    pub visualiser_settings: VisualiserSettings,
    pub active_device: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SettingsPatch {
    pub visualiser: Option<VisualisationType>,
    pub bands: Option<usize>,
    pub gain: Option<f32>,
    pub smoothing: Option<f32>,
    pub band_gap: Option<f32>,
    pub visualiser_settings: Option<VisualiserSettingsPatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceDto {
    pub name: String,
    pub formatted_name: String,
    pub supports_input: bool,
    pub supports_output: bool,
    pub device_type: String,
    pub direction: String,
    pub address: String,
    pub device_id: String,
    pub driver: String,
    pub manufacturer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectDeviceRequest {
    pub name: String,
}
