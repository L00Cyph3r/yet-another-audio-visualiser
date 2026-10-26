use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, DeviceId};
use log::{error, warn};
use std::str::FromStr;

pub fn get_audio_devices() -> Vec<Device> {
    let host = cpal::default_host();
    match host.devices() {
        Ok(devices) => devices.collect(),
        Err(e) => {
            error!("Failed to get audio devices: {}", e);
            Vec::new()
        }
    }
}
pub fn device_name_format(device: Option<&Device>) -> String {
    if device.is_some()
        && let Ok(description) = device.unwrap().description()
    {
        return format!(
            "{} - {} - {}",
            description.name(),
            description.driver().unwrap_or("Unknown"),
            description.interface_type()
        );
    }
    "Unknown device".to_string()
}

pub fn get_device_by_id(id: &str) -> Option<Device> {
    let host = cpal::default_host();
    match DeviceId::from_str(id) {
        Ok(parsed_id) => {
            if let Some(device) = host.device_by_id(&parsed_id) {
                return Some(device);
            } else {
                warn!("Device with ID {} not found", id);
            }
        }
        Err(e) => {
            error!("Failed to parse device ID: {}", e);
        }
    }
    None
}

pub fn get_default_device() -> Option<Device> {
    let host = cpal::default_host();
    host.default_input_device()
}
