//! Plugins use MethodChannel to interop with flutter/dart.
//! It contains two implementations StandardMethodChannel using binary encoding
//! and JsonMethodChannel using json encoding.

use crate::{FlutterEngine, FlutterEngineWeakRef};

pub use self::{
    message_channel::{Message, MessageChannel, MessageHandler},
    // event_channel::EventChannel,
    method_channel::{MethodCall, MethodCallHandler, MethodChannel, MethodError},
    registry::ChannelRegistry,
};
use crate::channel::platform_message::{PlatformMessage, PlatformMessageResponseHandle};

mod message_channel;
// TODO: Reimplement event channel support
// mod event_channel;
mod method_channel;
pub mod platform_message;
mod registry;

pub trait Channel {
    fn name(&self) -> &str;
    fn engine(&self) -> Option<FlutterEngine>;
    fn init(&mut self, engine: FlutterEngineWeakRef);
    fn handle_platform_message(&self, msg: PlatformMessage);

    /// When flutter call a method using MethodChannel,
    /// it can wait for rust response using await syntax.
    /// This method send a response to flutter. This is a low level method.
    fn send_response(&self, response_handle: PlatformMessageResponseHandle, buf: &[u8]) {
        if let Some(engine) = self.engine() {
            engine.send_platform_message_response(response_handle, buf);
        } else {
            log::error!("Channel {} was not initialized", self.name());
        }
    }
}
