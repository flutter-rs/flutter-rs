//! The system channels a Flutter app expects its embedder to answer.
//!
//! Each module here is one [`Plugin`](flutter_engine::plugins::Plugin): it
//! registers a channel with the engine and serves the calls the framework
//! makes on it. An embedder adds the ones it can serve to its
//! [`PluginRegistrar`](flutter_engine::plugins::PluginRegistrar), and the
//! plugins that need the host to do something — put text on the clipboard,
//! move the window, show the keyboard — take a handler trait for the embedder
//! to implement:
//!
//! ```ignore
//! use std::sync::Arc;
//! use parking_lot::Mutex;
//! use flutter_plugins::textinput::{TextInputHandler, TextInputPlugin};
//!
//! struct Keyboard;
//!
//! impl TextInputHandler for Keyboard {
//!     fn show(&mut self) {}
//!     fn hide(&mut self) {}
//! }
//!
//! plugins.add_plugin(
//!     &engine,
//!     TextInputPlugin::new(Arc::new(Mutex::new(Keyboard))),
//! );
//! ```
//!
//! (The examples here are not run: linking anything against this crate needs
//! `libflutter_engine` on the link path.)
//!
//! The plugins that push state the other way — the app's lifecycle, the
//! host's settings, the locale list — are driven by the embedder, not by the
//! framework: nothing asks for them, so an embedder that never sends them
//! leaves the framework with no idea it was ever resumed or that the user
//! wants a dark interface.
//!
//! Everything in a plugin handler runs on the engine's platform thread,
//! inside the engine's message dispatch.
//!
//! # Features
//!
//! - `dialog` (default): the `dialog` plugin, which vendors C sources through
//!   `cc`.

// Every public item in this crate is part of an embedder's contract with the
// framework, so none of them may go undocumented.
#![warn(missing_docs)]

pub mod accessibility;
#[cfg(feature = "dialog")]
pub mod dialog;
pub mod isolate;
pub mod keyboard;
#[deprecated(
    since = "0.5.0",
    note = "`flutter/keyevent` is the legacy RawKeyboard protocol, which current \
            Flutter no longer sends; send a FlutterKeyEvent through \
            FlutterEngine::send_key_event instead"
)]
pub mod keyevent;
pub mod lifecycle;
pub mod localization;
pub mod mousecursor;
pub mod navigation;
pub mod platform;
pub mod settings;
pub mod system;
pub mod textinput;
pub mod window;
