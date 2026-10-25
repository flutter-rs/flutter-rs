//! A Flutter engine embedder that renders into a GLFW window.
//!
//! [`init`] opens GLFW on the calling thread — which becomes the engine's
//! platform thread, and the only thread allowed to drive it — and
//! [`FlutterDesktop::create_window`] builds the window and the engine with the
//! system plugins (clipboard, text input, mouse cursor, locales, lifecycle,
//! navigation, …) already registered. [`window::FlutterWindow::run`] then owns
//! the thread until the window closes, translating GLFW's pointer, scroll and
//! key events into the engine's.
//!
//! `run` also reports the things the framework never asks for and would
//! otherwise default forever: the host's preferred locales, its light/dark
//! preference (once at startup and again whenever it changes), and the window's
//! `AppLifecycleState` as it gains and loses focus and visibility.

// Every `unsafe` block here calls into GLFW's C API, which has thread affinity
// rules no type can express, so each one must say why it is sound; and a
// published embedder wants its public API documented.
#![warn(clippy::undocumented_unsafe_blocks, missing_docs)]

use crate::window::{CreateError, FlutterWindow, WindowArgs};
use log::error;
use std::path::PathBuf;

mod handler;
mod keymap;
pub mod window;

/// Initialise GLFW, making the calling thread the engine's platform thread.
///
/// # Errors
///
/// Returns GLFW's [`InitError`](glfw::InitError) if the library is already
/// initialised or no display is available.
pub fn init() -> Result<FlutterDesktop, glfw::InitError> {
    glfw::init(glfw_error_callback).map(|glfw| FlutterDesktop { glfw })
}

fn glfw_error_callback(error: glfw::Error, description: String) {
    error!("GLFW error ({}): {}", error, description);
}

/// An initialised GLFW library, from which Flutter windows are created.
pub struct FlutterDesktop {
    glfw: glfw::Glfw,
}

impl FlutterDesktop {
    /// Create a window and an engine that renders `assets_path`'s Flutter
    /// app into it.
    ///
    /// # Errors
    ///
    /// Returns [`CreateError`] if GLFW cannot create the window or the engine
    /// cannot be built.
    pub fn create_window(
        &mut self,
        window_args: &WindowArgs,
        assets_path: PathBuf,
        arguments: Vec<String>,
    ) -> Result<FlutterWindow, CreateError> {
        FlutterWindow::create(&mut self.glfw, window_args, assets_path, None, arguments)
    }

    /// Like [`create_window`](Self::create_window) but also points the engine
    /// at an external `icudtl.dat`, required by official engine builds that no
    /// longer link the ICU data into `libflutter_engine`.
    ///
    /// # Errors
    ///
    /// Returns [`CreateError`] if GLFW cannot create the window or the engine
    /// cannot be built.
    pub fn create_window_with_icu(
        &mut self,
        window_args: &WindowArgs,
        assets_path: PathBuf,
        icu_data_path: PathBuf,
        arguments: Vec<String>,
    ) -> Result<FlutterWindow, CreateError> {
        FlutterWindow::create(
            &mut self.glfw,
            window_args,
            assets_path,
            Some(icu_data_path),
            arguments,
        )
    }

    /// A handle to the underlying GLFW library, for window hints and other
    /// GLFW state this crate does not wrap.
    #[must_use]
    pub fn glfw(&self) -> glfw::Glfw {
        self.glfw.clone()
    }
}
