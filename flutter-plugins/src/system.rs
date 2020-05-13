//! Plugin to work with locales.
//! It handles flutter/localization type message.
use std::sync::Weak;

use serde::{Deserialize, Serialize};

use flutter_engine::{
    channel::{MessageChannel, MessageHandler},
    codec::JSON_CODEC,
    plugins::Plugin,
    FlutterEngine,
};

use flutter_engine::channel::Message;
use flutter_engine::codec::Value;
use log::{error, info};

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/system";

pub struct SystemPlugin {
    channel: Weak<MessageChannel>,
}

impl Default for SystemPlugin {
    fn default() -> Self {
        Self {
            channel: Weak::new(),
        }
    }
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
        msg.respond(Value::Null)
    }
}
