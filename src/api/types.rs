use crate::visualisations::VisualisationType;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum AppCommand {
    UpdateSettings(SettingsPatch, Option<tokio::sync::oneshot::Sender<SettingsDto>>),
    SelectDevice(String, Option<tokio::sync::oneshot::Sender<Result<(), String>>>),
    GetSettings(tokio::sync::oneshot::Sender<SettingsDto>),
    GetDevices(tokio::sync::oneshot::Sender<Vec<DeviceDto>>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SettingsDto {
    pub visualiser: VisualisationType,
    pub bands: usize,
    pub gain: f32,
    pub smoothing: f32,
    pub band_gap: f32,
    pub active_device: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SettingsPatch {
    pub visualiser: Option<VisualisationType>,
    pub bands: Option<usize>,
    pub gain: Option<f32>,
    pub smoothing: Option<f32>,
    pub band_gap: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceDto {
    pub name: String,
    pub formatted_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectDeviceRequest {
    pub name: String,
}
