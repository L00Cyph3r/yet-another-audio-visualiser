//! Profiling instrumentation, enabled with the `profiling` cargo feature.
//!
//! Everything profiler-specific is funnelled through this module, so the rest of the code base
//! only ever mentions [`profile_function!`], [`profile_scope!`] and the three functions below.
//!
//! With the `profiling` feature enabled, the macros forward to the [`puffin`] profiler and
//! [`start_puffin_server`] opens the TCP server that `puffin_viewer` connects to. Without the
//! feature the macros expand to nothing and the functions are no-ops, so a normal build is
//! completely unaffected.
//!
//! Scopes are only recorded while scope collection is on, which [`start_puffin_server`] turns on.
//!
//! # Seeing the profile
//!
//! Externally: `cargo install puffin_viewer` and run
//! `puffin_viewer --url 127.0.0.1:8585`.
//!
//! Note that the realtime audio callback in [`crate::capture`] is deliberately *not*
//! instrumented: it must not allocate or block. Use a sampling profiler such as `samply` to see
//! that thread.

#[cfg(feature = "profiling")]
pub(crate) use puffin::{profile_function, profile_scope};

#[cfg(not(feature = "profiling"))]
macro_rules! profile_function {
    () => {};
}

#[cfg(not(feature = "profiling"))]
pub(crate) use profile_function;

#[cfg(not(feature = "profiling"))]
macro_rules! profile_scope {
    ($($arg:tt)*) => {};
}

#[cfg(not(feature = "profiling"))]
pub(crate) use profile_scope;

/// Port the puffin TCP server listens on, for `puffin_viewer`.
#[cfg(feature = "profiling")]
pub const PUFFIN_VIEWER_PORT: u16 = 8585;

/// Turns on scope collection and starts the TCP server that `puffin_viewer` connects to.
///
/// Does nothing when the `profiling` feature is disabled.
#[cfg(feature = "profiling")]
pub fn start_puffin_server() {
    puffin::set_scopes_on(true);

    let addr = format!("127.0.0.1:{PUFFIN_VIEWER_PORT}");
    match puffin_http::Server::new(&addr) {
        Ok(server) => {
            // Dropping the server shuts it down, so it has to outlive this call.
            std::mem::forget(server);
            log::info!(
                "Puffin profiler listening on {addr}; \
                 run `cargo install puffin_viewer` and `puffin_viewer --url {addr}` to inspect it"
            );
        }
        Err(err) => log::error!("Failed to start the puffin profiler server: {err}"),
    }
}

/// No-op counterpart of [`start_puffin_server`] for builds without the `profiling` feature.
#[cfg(not(feature = "profiling"))]
pub fn start_puffin_server() {}

/// Marks the start of a frame, handing the scopes recorded during the previous one to the sinks.
///
/// Must be called exactly once per frame, before any other scope is opened.
#[cfg(feature = "profiling")]
pub fn end_frame() {
    puffin::GlobalProfiler::lock().new_frame();
}

/// No-op counterpart of [`end_frame`] for builds without the `profiling` feature.
#[cfg(not(feature = "profiling"))]
pub fn end_frame() {}
