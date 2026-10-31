//! Library half of the audio visualiser.
//!
//! The application logic lives here so that it can be reused by the binary (`src/main.rs`),
//! by the integration tests and by the benchmarks in `benches/`. `src/main.rs` is only a thin
//! wrapper that parses the command line, initialises logging/error reporting and starts
//! `eframe`.

pub mod analyzer;
pub mod api;
pub mod capture;
pub mod gui;
pub mod profiling;
pub mod utils;
pub mod visualisations;
