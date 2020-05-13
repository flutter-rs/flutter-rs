//! Plugin to work with locales.
//! It handles flutter/localization type message.

use log::debug;
use std::sync::Weak;

use flutter_engine::{
    channel::{MessageChannel, MessageHandler},
    codec::STRING_CODEC,
    plugins::Plugin,
    FlutterEngine,
};

use flutter_engine::channel::Message;
use flutter_engine::codec::Value;

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/lifecycle";

pub struct LifecyclePlugin {
    channel: Weak<MessageChannel>,
}

impl Default for LifecyclePlugin {
    fn default() -> Self {
        Self {
            channel: Weak::new(),
        }
    }
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
    pub fn send_app_is_inactive(&self) {
        debug!("Sending app is inactive");
        if let Some(channel) = self.channel.upgrade() {
            channel.send("AppLifecycleState.inactive");
        }
    }

    pub fn send_app_is_resumed(&self) {
        debug!("Sending app is resumed");
        if let Some(channel) = self.channel.upgrade() {
            channel.send("AppLifecycleState.resumed");
        }
    }

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
        msg.respond(Value::Null)
    }
}
