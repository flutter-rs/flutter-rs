//! Plugin for the `flutter/system` channel.
//!
//! Sends system notifications to the framework; the only one the framework
//! reads is `memoryPressure`, which makes it drop its caches.
use std::sync::Weak;

use serde::{Deserialize, Serialize};

use flutter_engine::{
    FlutterEngine,
    channel::{MessageChannel, MessageHandler},
    codec::JSON_CODEC,
    plugins::Plugin,
};

use flutter_engine::channel::Message;
use flutter_engine::codec::Value;
use log::{error, info};

/// The name this plugin is registered under.
pub const PLUGIN_NAME: &str = module_path!();
/// The channel this plugin handles.
pub const CHANNEL_NAME: &str = "flutter/system";

/// Plugin for the `flutter/system` channel.
#[derive(Default)]
pub struct SystemPlugin {
    channel: Weak<MessageChannel>,
}

impl Plugin for SystemPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        self.channel =
            engine.register_channel(MessageChannel::new(CHANNEL_NAME, Handler, &JSON_CODEC));
    }
}

impl SystemPlugin {
    /// Ask the framework to release what memory it can.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped. Can be called from any thread: the send hops onto the
    /// platform thread by itself.
    pub fn send_memory_pressure_warning(&self) {
        info!("Sending memory pressure warning");
        if let Some(channel) = self.channel.upgrade() {
            channel.send(SystemMsg {
                r#type: "memoryPressure".to_string(),
            });
        } else {
            error!("Failed to upgrade channel to send memory pressure warning");
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SystemMsg {
    r#type: String,
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
