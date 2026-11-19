//! A Flutter engine embedder that renders into GLFW windows.
//!
//! [`init`] opens GLFW on the calling thread — which becomes the engine's
//! platform thread, and the only thread allowed to drive it.
//! [`FlutterDesktop::create_app`] builds the one engine and its hidden render
//! contexts, [`FlutterApp::create_window`] adds native windows and engine
//! views, and [`FlutterApp::run`] owns the thread until the app exits,
//! translating every window's pointer, scroll and key events into the engine's.
//!
//! The app also reports the things the framework never asks for and would
//! otherwise default forever: the host's preferred locales, its light/dark
//! preference, and the app's `AppLifecycleState` as its windows gain and lose
//! focus and visibility.

// Every `unsafe` block here calls into GLFW's C API, which has thread affinity
// rules no type can express, so each one must say why it is sound; and a
// published embedder wants its public API documented.
#![warn(clippy::undocumented_unsafe_blocks, missing_docs)]

use log::error;

mod app;
mod compositor;
mod handler;
mod keymap;
#[cfg(target_os = "macos")]
mod metal;
mod render;
pub mod window;
mod windowing;

pub use app::{AppArgs, FlutterApp};
pub use window::{
    CreateError, FlutterWindow, PerFrameCallback, RunError, WindowArgs, WindowEventHandler,
    WindowMode,
};

/// Initialise GLFW, making the calling thread the engine's platform thread.
///
/// `FLUTTER_GLFW_PLATFORM=x11|wayland` forces the GLFW platform before
/// initialisation, which is useful on a Wayland session where the embedder
/// needs X11/XWayland's window positioning.
///
/// # Errors
///
/// Returns GLFW's [`InitError`](glfw::InitError) if the library is already
/// initialised or no display is available.
pub fn init() -> Result<FlutterDesktop, glfw::InitError> {
    if let Ok(platform) = std::env::var("FLUTTER_GLFW_PLATFORM") {
        let platform = match platform.to_ascii_lowercase().as_str() {
            "x11" => Some(glfw::Platform::X11),
            "wayland" => Some(glfw::Platform::Wayland),
            _ => None,
        };
        if let Some(platform) = platform {
            glfw::init_hint(glfw::InitHint::Platform(platform));
        }
    }
    glfw::init(glfw_error_callback).map(|glfw| FlutterDesktop { glfw })
}

fn glfw_error_callback(error: glfw::Error, description: String) {
    error!("GLFW error ({}): {}", error, description);
}

/// An initialised GLFW library, from which Flutter apps are created.
pub struct FlutterDesktop {
    glfw: glfw::Glfw,
}

impl FlutterDesktop {
    /// Create the app: one Flutter engine, its hidden master/resource contexts,
    /// and the system plugins (clipboard, text input, mouse cursor, locales,
    /// lifecycle, navigation, …).
    ///
    /// Windows are created afterwards with [`FlutterApp::create_window`].
    ///
    /// # Errors
    ///
    /// Returns [`CreateError`] if GLFW cannot create the hidden contexts or the
    /// engine cannot be built.
    pub fn create_app(&mut self, args: AppArgs) -> Result<FlutterApp, CreateError> {
        let context_api = args.context_api.unwrap_or_else(default_context_api);
        FlutterApp::create(self.glfw.clone(), args, context_api)
    }

    /// A handle to the underlying GLFW library, for window hints and other
    /// GLFW state this crate does not wrap.
    #[must_use]
    pub fn glfw(&self) -> glfw::Glfw {
        self.glfw.clone()
    }
}

/// EGL is the working backend on Linux; Windows and macOS use their native
/// context APIs unless the caller overrides it (a Windows Mesa/llvmpipe host
/// may need `ContextCreationApi::Egl`).
fn default_context_api() -> glfw::ContextCreationApi {
    if cfg!(target_os = "linux") {
        glfw::ContextCreationApi::Egl
    } else {
        glfw::ContextCreationApi::Native
    }
}
