//! Plugin for the `flutter/lifecycle` channel.
//!
//! Pushes `AppLifecycleState` transitions to the framework, which is what
//! drives `didChangeAppLifecycleState` and `AppLifecycleListener`. Nothing
//! asks for them: the embedder has to send them as the host window gains and
//! loses focus, and once at startup, or the framework's lifecycle state stays
//! `null` for the whole run.

use log::debug;
use std::sync::Weak;

use flutter_engine::{
    FlutterEngine,
    channel::{MessageChannel, MessageHandler},
    codec::STRING_CODEC,
    plugins::Plugin,
};

use flutter_engine::channel::Message;
use flutter_engine::codec::Value;

/// The name this plugin is registered under.
pub const PLUGIN_NAME: &str = module_path!();
/// The channel this plugin handles.
pub const CHANNEL_NAME: &str = "flutter/lifecycle";

/// Plugin for the `flutter/lifecycle` channel.
#[derive(Default)]
pub struct LifecyclePlugin {
    channel: Weak<MessageChannel>,
}

impl Plugin for LifecyclePlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        self.channel =
            engine.register_channel(MessageChannel::new(CHANNEL_NAME, Handler, &STRING_CODEC));
    }
}

impl LifecyclePlugin {
    /// Notify the framework that the app is visible but not focused.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped. Can be called from any thread: the send hops onto the
    /// platform thread by itself.
    pub fn send_app_is_inactive(&self) {
        debug!("Sending app is inactive");
        if let Some(channel) = self.channel.upgrade() {
            channel.send("AppLifecycleState.inactive");
        }
    }

    /// Notify the framework that the app is visible and focused.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped. Can be called from any thread: the send hops onto the
    /// platform thread by itself.
    pub fn send_app_is_resumed(&self) {
        debug!("Sending app is resumed");
        if let Some(channel) = self.channel.upgrade() {
            channel.send("AppLifecycleState.resumed");
        }
    }

    /// Notify the framework that the app is not visible.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped. Can be called from any thread: the send hops onto the
    /// platform thread by itself.
    pub fn send_app_is_paused(&self) {
        debug!("Sending app is paused");
        if let Some(channel) = self.channel.upgrade() {
            channel.send("AppLifecycleState.paused");
        }
    }
}

struct Handler;

impl MessageHandler for Handler {
    fn on_message(&mut self, msg: Message) {
        // The framework does not send on this channel, but a reply it is not
        // waiting for would only be logged as an error.
        if msg.can_respond() {
            msg.respond(Value::Null);
        }
    }
}
