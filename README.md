# yet-another-audio-visualiser

[![Release](https://github.com/L00Cyph3r/yet-another-audio-visualiser/actions/workflows/release.yml/badge.svg)](https://github.com/L00Cyph3r/yet-another-audio-visualiser/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](#license)
![Rust](https://img.shields.io/badge/rust-2024%20edition-orange.svg)

A cross-platform desktop audio visualiser written in Rust. It captures audio from any input
device, turns it into a real-time spectrum with an FFT, and renders it through a set of
configurable visualisations — all driven by an egui GUI **and** a local HTTP API so external
scripts, stream decks, or widgets can change settings on the fly.

## Key Features

- **Real-time spectrum analysis** — captures PCM from any input device, mixes it down to mono,
  and runs a windowed FFT (`rustfft`) with logarithmically spaced frequency bands, dB mapping,
  and fast-attack / slow-release smoothing.
- **Four visualisations**, each with its own live settings editor:
  - **Equalizer** — bar graph with optional colour-by-volume, custom colour, corner radius, band count and gaps.
  - **Waveform** — filled bars with configurable colour, radius, bands and gaps.
  - **Nyan** — a Nyan Cat sprite flying along a rainbow trail built from the spectrum history.
  - **Segmented Spectrum Bars** — LED-style bars split into stacked segments, with selectable
    colour mappings (`LED`, `Gradient`, `Stepped`, `Rainbow`, `Solid`) and editable colour stops.
- **Device management** — lists available input devices and switches the active capture source at runtime.
- **Persistent settings** — configuration is saved to a JSON file in the platform config directory
  (written atomically, with corrupt files moved aside), and restored on the next launch.
- **Local HTTP REST API** — read and patch settings or switch devices from `127.0.0.1:3000`
  while the app is running, with changes reflected immediately in the GUI.
- **Built-in error reporting** — panics and errors are reported to Sentry.
- **Cross-platform release builds** — a GitHub Actions workflow produces Windows, macOS and Linux artifacts.

## Tech Stack & Tools

| Area            | Choice                     | Why                                                                                                  |
|-----------------|----------------------------|------------------------------------------------------------------------------------------------------|
| Language        | **Rust** (edition 2024)    | Memory safety and native performance for real-time audio/FFT work.                                   |
| GUI             | **eframe / egui 0.37**     | Immediate-mode GUI that is simple to drive from a render loop and keeps state in plain Rust structs. |
| Audio capture   | **cpal 0.18**              | Cross-platform audio I/O (WASAPI on Windows, ALSA/PipeWire on Linux, CoreAudio on macOS).            |
| DSP             | **rustfft 6.4**            | Fast, pure-Rust FFT for the spectrum analysis.                                                       |
| HTTP API        | **axum 0.8** + **tokio 1** | Ergonomic async HTTP server running on a background thread, isolated from the GUI.                   |
| Messaging       | **crossbeam-channel**      | Lock-free channels to move commands between the async server and the GUI thread.                     |
| Serialization   | **serde / serde_json**     | Settings and API payloads are plain JSON DTOs.                                                       |
| Enums           | **strum**                  | Iterating over the visualisation types to build the selector.                                        |
| Paths           | **dirs**                   | Locating the platform configuration directory.                                                       |
| Logging         | **log** + **env_logger**   | Structured application logging to the console.                                                       |
| Images          | **image** (PNG)            | Decoding the embedded Nyan Cat sprite.                                                               |
| Error reporting | **sentry 0.49**            | Captures panics and errors.                                                                          |
| Error handling  | **anyhow**                 | Ergonomic error propagation in the audio capture path.                                               |

## Architecture & Project Structure

```
src/
├── main.rs                     # Entry point: Sentry init, settings load, eframe run, API server start
├── analyzer.rs                 # SpectrumAnalyzer: windowed FFT -> log-spaced bands -> smoothing
├── capture.rs                  # AudioCapture: cpal input stream -> mono sample channel
├── utils.rs                    # Audio device helpers (list, format, default)
├── api/
│   ├── mod.rs
│   ├── server.rs               # Axum router + handlers, background Tokio runtime
│   └── types.rs                # AppCommand, SettingsDto, SettingsPatch, DeviceDto
├── gui/
│   ├── mod.rs
│   ├── app.rs                  # GuiApp: egui UI, command handling, persistence, viewports
│   └── settings.rs             # AppSettings + SettingsStore (atomic JSON persistence)
└── visualisations/
    ├── mod.rs                  # Visualisation trait, VisualisationType, shared settings structs
    ├── equalizer.rs
    ├── waveform.rs
    ├── nyan.rs
    └── segmented_spectrum_bars.rs
```

**Data flow**

1. **Audio** — `cpal` runs a background input stream (`capture.rs`) that downmixes frames to mono
   and pushes `f32` samples through a bounded `crossbeam-channel`.
2. **Analysis** — each frame, `GuiApp` drains the channel into a `SpectrumAnalyzer` (`analyzer.rs`),
   which applies a Hann window, runs the FFT, groups bins into logarithmic bands, converts to dB,
   and smooths the result.
3. **Rendering** — `GuiApp::ui()` reads the band levels and calls `Visualisation::draw` for the
   selected type, rendering into an egui viewport.
4. **Control** — the HTTP API (`api/server.rs`) runs on its own thread and sends `AppCommand`s over
   a `crossbeam-channel`. `GuiApp::handle_commands()` applies them at the start of each frame and
   calls `request_repaint()` so changes show up immediately.

## Getting Started / Installation

### Prerequisites

- **Rust** — a stable toolchain with **edition 2024** support (Rust **1.85+**). Install via [rustup](https://rustup.rs/).
- **An audio input device** (microphone, line-in, loopback, etc.).

Platform notes:

- **Windows** — no extra system dependencies.
- **macOS** — no extra system dependencies.
- **Linux** — the audio backend uses ALSA + PipeWire, and the GUI needs the usual X11/Wayland/GTK
  headers. Install them first:

  ```bash
  sudo apt-get update && sudo apt-get install -y \
    pkg-config \
    libasound2-dev \
    libpipewire-0.3-dev \
    libspa-0.2-dev \
    libxkbcommon-dev \
    libxcb-render0-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    libwayland-dev \
    libssl-dev \
    libgtk-3-dev \
    clang \
    libclang-dev
  ```

  > **Note:** the `pipewire`/`libspa` crates generate bindings from the system PipeWire headers and
  > require **PipeWire ≥ 1.0** (Ubuntu **24.04+**). Older distributions such as Ubuntu 22.04 ship
  > PipeWire 0.3.48 and will fail to compile `libspa`.

### Install & Run

```bash
git clone https://github.com/L00Cyph3r/yet-another-audio-visualiser.git
cd yet-another-audio-visualiser

# Debug build + run
cargo run

# Optimised build + run (recommended for smooth visuals)
cargo run --release
```

To build a distributable binary:

```bash
cargo build --release
# binary: target/release/yet-another-audio-visualiser(.exe)
```

### Prebuilt binaries

Every [release](https://github.com/L00Cyph3r/yet-another-audio-visualiser/releases) ships one
archive per platform. Pick the file that matches your OS (the macOS build is a universal binary
that runs natively on both Apple Silicon and Intel Macs):

| Platform | Architecture                      | Download                                                                 |
|----------|-----------------------------------|--------------------------------------------------------------------------|
| Windows  | x86_64                            | `yet-another-audio-visualiser-<version>-windows-x86_64.zip`              |
| macOS    | Universal (Apple Silicon + Intel) | `yet-another-audio-visualiser-<version>-macos-universal.tar.gz`          |
| Linux    | x86_64                            | `yet-another-audio-visualiser-<version>-linux-x86_64.tar.gz` (or `.deb`) |
| Linux    | arm64                             | `yet-another-audio-visualiser-<version>-linux-arm64.tar.gz` (or `.deb`)  |

Each archive has a matching `.sha256` file so you can verify its integrity, e.g.:

```bash
sha256sum -c yet-another-audio-visualiser-0.3.0-linux-x86_64.tar.gz.sha256
```

The macOS downloads are `.app` bundles (unpack the `.tar.gz` and drag the app into
`/Applications`). They are not code-signed, so macOS may warn about an "unidentified developer"
on first launch — right-click the app and choose **Open**.

### Tests

```bash
cargo test
```

## Usage

### GUI

Launch the app, then:

1. Pick your **Audio Input** from the dropdown (the system default is used if none is saved).
2. Choose a **Visualisation** from the dropdown — its settings appear below and are saved as you edit them.
3. The visualisation is rendered in a separate **“Visualisation”** window; the main window holds the controls.
   The visualisation window has no controls of its own, so you can move it by pressing and dragging
   with the left mouse button anywhere inside it.

Keyboard shortcuts:

| Key   | Action                                   |
|-------|------------------------------------------|
| `F1`  | Toggle the help text                     |
| `Esc` | Stop capture and clear the active device |

### HTTP API

The app exposes a local REST API on `http://127.0.0.1:3000` while running.

| Method  | Endpoint              | Description                                                                                          |
|---------|-----------------------|------------------------------------------------------------------------------------------------------|
| `GET`   | `/api/settings`       | Current settings (active visualisation, gain, smoothing, per-visualisation settings, active device). |
| `PATCH` | `/api/settings`       | Partially update settings.                                                                           |
| `GET`   | `/api/devices`        | List available input devices.                                                                        |
| `POST`  | `/api/devices/select` | Switch the active input device.                                                                      |

Examples:

```bash
# Read the current settings
curl http://127.0.0.1:3000/api/settings

# Switch visualisation and adjust shared settings
curl -X PATCH http://127.0.0.1:3000/api/settings \
  -H "Content-Type: application/json" \
  -d '{"visualiser":"Waveform","gain":2.5,"smoothing":0.3}'

# Patch per-visualisation settings (only the mentioned fields change)
curl -X PATCH http://127.0.0.1:3000/api/settings \
  -H "Content-Type: application/json" \
  -d '{"visualiser_settings":{"segmented_spectrum_bars":{"segment_count":16,"color_mapping":"Rainbow"}}}'

# List devices, then select one by name
curl http://127.0.0.1:3000/api/devices
curl -X POST http://127.0.0.1:3000/api/devices/select \
  -H "Content-Type: application/json" \
  -d '{"name":"<device name>"}'
```

Notes:

- `PATCH` is **partial** — omitted fields keep their current values, and out-of-range values are
  clamped instead of rejected. Unknown settings return `400 Bad Request`.
- Per-visualisation options (including `bands`) live under `visualiser_settings.<section>`, where
  `<section>` is one of `equalizer`, `waveform`, `nyan`, or `segmented_spectrum_bars`.
- The `visualiser` field accepts either a plain name (e.g. `"Waveform"`, which uses that type's
  defaults) or an object carrying explicit settings (e.g. `{"Waveform":{"bands":64}}`).

### Configuration file

Settings are stored as JSON and loaded automatically on launch:

| Platform | Location                                                                   |
|----------|----------------------------------------------------------------------------|
| Windows  | `%APPDATA%\yet-another-audio-visualiser\settings.json`                     |
| macOS    | `~/Library/Application Support/yet-another-audio-visualiser/settings.json` |
| Linux    | `~/.config/yet-another-audio-visualiser/settings.json`                     |

The file is written atomically, and an unreadable file is moved to `settings.json.corrupt` and
replaced with defaults.

## Contributing

Contributions are welcome:

1. **Issues** — open an issue describing the bug or feature, including steps to reproduce and your
   platform (OS, Rust version, audio device) where relevant.
2. **Pull requests** — fork the repository, create a feature branch, and keep changes focused.
   Before opening a PR, please make sure the project is formatted and the tests pass:

   ```bash
   cargo fmt
   cargo test
   cargo build --release
   ```

   Describe what changed and why in the PR description.

Releases are produced automatically by the [`release`](.github/workflows/release.yml) workflow when
a tag matching `v*` is pushed.

## License

Licensed under the **MIT License** (see the `license` field in [`Cargo.toml`](Cargo.toml)).
