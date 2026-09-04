use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Devices, Error};

pub fn get_audio_devices() -> Vec<Device> {
    let host = cpal::default_host();
    match host.devices() {
        Ok(devices) => devices.collect(),
        Err(e) => {
            eprintln!("Failed to get audio devices");
            eprintln!("Error: {}", e);
            Vec::new()
        }
    }
}
pub fn device_name_format(device: Option<&cpal::Device>) -> String {
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
