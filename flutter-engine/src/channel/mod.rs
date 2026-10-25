//! Platform channels: how Rust and Dart talk to each other.
//!
//! A [`MethodChannel`] carries method calls and their replies (Dart's
//! `MethodChannel`), a [`MessageChannel`] carries plain values in both
//! directions (Dart's `BasicMessageChannel`). Both are registered with the
//! engine through
//! [`FlutterEngine::register_channel`](crate::FlutterEngine::register_channel),
//! which puts them in the engine's [`ChannelRegistry`].
//!
//! Everything in this module happens on the platform thread: the engine
//! dispatches incoming messages there, and channels send from there. Handler
//! code therefore runs inside an `extern "C"` engine callback, which is why
//! nothing here panics on bad input from Dart — a panic cannot unwind out of
//! that callback.

use crate::{FlutterEngine, FlutterEngineWeakRef};

pub use self::{
    message_channel::{Message, MessageChannel, MessageHandler},
    method_channel::{MethodCall, MethodCallHandler, MethodChannel, MethodError},
    registry::ChannelRegistry,
};
use crate::channel::platform_message::{PlatformMessage, PlatformMessageResponseHandle};

mod message_channel;
// TODO: Reimplement event channel support. `FlutterEngine::post_message`
// wrapped in `run_on_platform_thread` is the supported way to push events to
// Dart until then.
mod method_channel;
pub mod platform_message;
mod registry;

/// One registered channel.
///
/// Implemented by [`MethodChannel`] and [`MessageChannel`]; implement it
/// yourself only for a channel with a wire protocol neither of those fits.
pub trait Channel {
    /// The channel name, as Dart knows it.
    fn name(&self) -> &str;

    /// The engine the channel was registered with, if it is still alive.
    fn engine(&self) -> Option<FlutterEngine>;

    /// Called by the registry when the channel is registered.
    fn init(&mut self, engine: FlutterEngineWeakRef);

    /// Called on the platform thread for every message Dart sends on this
    /// channel.
    ///
    /// If `msg` carries a response handle, this must answer it — directly or
    /// by handing it to code that will — or the engine leaks the memory behind
    /// it.
    fn handle_platform_message(&self, msg: PlatformMessage);

    /// Send a raw, already-encoded response.
    ///
    /// This is the low-level primitive the typed `respond` methods are built
    /// on; an empty `buf` is what the framework reads as "no plugin implements
    /// this".
    fn send_response(&self, response_handle: PlatformMessageResponseHandle, buf: &[u8]) {
        if let Some(engine) = self.engine() {
            engine.send_platform_message_response(response_handle, buf);
        } else {
            log::error!("Channel {} was not initialized", self.name());
        }
    }
}
