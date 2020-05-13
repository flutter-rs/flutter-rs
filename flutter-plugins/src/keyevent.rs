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

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/keyevent";

pub struct KeyEventPlugin {
    channel: Weak<MessageChannel>,
}

struct Handler;

impl Plugin for KeyEventPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        self.channel =
            engine.register_channel(MessageChannel::new(CHANNEL_NAME, Handler, &JSON_CODEC));
    }
}

impl Default for KeyEventPlugin {
    fn default() -> Self {
        Self {
            channel: Weak::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct KeyAction {
    pub toolkit: String,
    #[serde(rename = "keyCode")]
    pub key_code: i32,
    #[serde(rename = "scanCode")]
    pub scan_code: i32,
    pub modifiers: i32,
    pub keymap: String,
    #[serde(rename = "type")]
    pub _type: KeyActionType,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyActionType {
    Keydown,
    Keyup,
}

impl KeyEventPlugin {
    fn with_channel<F>(&self, f: F)
    where
        F: FnOnce(&MessageChannel),
    {
        if let Some(channel) = self.channel.upgrade() {
            f(&*channel);
        }
    }

    pub fn key_action(&self, action: KeyAction) {
        self.with_channel(|channel| {
            channel.send(action);
        });
    }
}

impl MessageHandler for Handler {
    fn on_message(&mut self, msg: Message) {
        msg.respond(Value::Null)
    }
}
