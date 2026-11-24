//! Plugin for the `flutter/accessibility` channel.
//! The framework posts accessibility events (announcements, focus changes,
//! tooltips, ...) here as a `BasicMessageChannel`. We have no screen-reader
//! integration, so we simply acknowledge the messages to complete the
//! framework-side send and otherwise ignore them.
use std::sync::Weak;

use flutter_engine::{
    FlutterEngine,
    channel::{Message, MessageChannel, MessageHandler},
    codec::{STANDARD_CODEC, Value},
    plugins::Plugin,
};
use log::trace;

/// The name this plugin is registered under.
pub const PLUGIN_NAME: &str = module_path!();
/// The channel this plugin handles.
pub const CHANNEL_NAME: &str = "flutter/accessibility";

/// Plugin for the `flutter/accessibility` channel.
#[derive(Default)]
pub struct AccessibilityPlugin {
    channel: Weak<MessageChannel>,
}

impl Plugin for AccessibilityPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        self.channel =
            engine.register_channel(MessageChannel::new(CHANNEL_NAME, Handler, &STANDARD_CODEC));
    }
}

struct Handler;

impl MessageHandler for Handler {
    fn on_message(&mut self, msg: Message) {
        trace!("accessibility event: {:?}", msg.raw_value());
        if msg.can_respond() {
            msg.respond(Value::Null);
        }
    }
}
