//! Plugin to work with settings.
//! It handles flutter/settings type message.

use std::collections::HashMap;

use log::debug;
use std::sync::Weak;

use serde::{Deserialize, Serialize};

use flutter_engine::{
    channel::{MessageChannel, MessageHandler},
    codec::JSON_CODEC,
    plugins::Plugin,
    FlutterEngine,
};

use flutter_engine::channel::Message;
use flutter_engine::codec::value::to_value;
use flutter_engine::codec::Value;

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/settings";

pub struct SettingsPlugin {
    channel: Weak<MessageChannel>,
}

pub struct SettingsMessage<'a> {
    plugin: &'a SettingsPlugin,
    settings: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlatformBrightness {
    Light,
    Dark,
}

impl Default for SettingsPlugin {
    fn default() -> Self {
        Self {
            channel: Weak::new(),
        }
    }
}

impl Plugin for SettingsPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        self.channel =
            engine.register_channel(MessageChannel::new(CHANNEL_NAME, Handler, &JSON_CODEC));
    }
}

impl SettingsMessage<'_> {
    pub fn set_text_scale_factor(mut self, factor: f64) -> Self {
        self.settings
            .insert("textScaleFactor".into(), Value::F64(factor));
        self
    }

    pub fn set_use_24_hour_format(mut self, use_24_hour_format: bool) -> Self {
        self.settings.insert(
            "alwaysUse24HourFormat".into(),
            Value::Boolean(use_24_hour_format),
        );
        self
    }

    pub fn set_platform_brightness(mut self, brightness: PlatformBrightness) -> Self {
        self.settings
            .insert("platformBrightness".into(), to_value(brightness).unwrap());
        self
    }

    pub fn send(self) {
        if let Some(channel) = self.plugin.channel.upgrade() {
            debug!("Sending settings: {:?}", self.settings);
            channel.send(self.settings);
        }
    }
}

impl SettingsPlugin {
    pub fn start_message(&self) -> SettingsMessage {
        debug!("Starting to build message");
        SettingsMessage {
            plugin: self,
            settings: HashMap::new(),
        }
    }
}

struct Handler;

impl MessageHandler for Handler {
    fn on_message(&mut self, msg: Message) {
        msg.respond(Value::Null)
    }
}
