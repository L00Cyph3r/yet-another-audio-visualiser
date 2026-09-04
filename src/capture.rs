use anyhow::Result;
use cpal::Device;
use cpal::traits::{DeviceTrait, StreamTrait};
use crossbeam_channel::{Receiver, bounded};

pub struct AudioCapture {
    // Must stay alive for as long as you want to capture audio.
    _stream: cpal::Stream,
    pub samples: Receiver<f32>,
    pub sample_rate: u32,
    pub device_name: String,
}

pub fn start_capture(device: Device) -> Result<AudioCapture> {
    println!("Using input device: {}", device.description()?.name());

    let supported_config = device.default_input_config()?;
    let sample_rate = supported_config.sample_rate();
    let channels = supported_config.channels() as usize;

    let (tx, rx) = bounded::<f32>(16_384);

    let err_fn = |err| eprintln!("Audio stream error: {err}");

    let stream = match supported_config.sample_format() {
        cpal::SampleFormat::F32 => {
            let tx = tx.clone();
            device.build_input_stream(
                supported_config.config(),
                move |data: &[f32], _| {
                    // Convert interleaved stereo/multichannel to mono.
                    for frame in data.chunks(channels) {
                        let sample = frame.iter().copied().sum::<f32>() / frame.len() as f32;

                        // Don't block the realtime audio callback.
                        let _ = tx.try_send(sample);
                    }
                },
                err_fn,
                None,
            )?
        }

        cpal::SampleFormat::I16 => {
            let tx = tx.clone();
            device.build_input_stream(
                supported_config.config(),
                move |data: &[i16], _| {
                    for frame in data.chunks(channels) {
                        let sample = frame
                            .iter()
                            .map(|&s| s as f32 / i16::MAX as f32)
                            .sum::<f32>()
                            / frame.len() as f32;

                        let _ = tx.try_send(sample);
                    }
                },
                err_fn,
                None,
            )?
        }

        cpal::SampleFormat::U16 => {
            let tx = tx.clone();
            device.build_input_stream(
                supported_config.config(),
                move |data: &[u16], _| {
                    for frame in data.chunks(channels) {
                        let sample = frame
                            .iter()
                            .map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0)
                            .sum::<f32>()
                            / frame.len() as f32;

                        let _ = tx.try_send(sample);
                    }
                },
                err_fn,
                None,
            )?
        }

        format => anyhow::bail!("Unsupported sample format: {format:?}"),
    };

    stream.play()?;

    Ok(AudioCapture {
        _stream: stream,
        samples: rx,
        sample_rate,
        device_name: device.description()?.name().to_string(),
    })
}
