//! Plugin for the `flutter/settings` channel.
//!
//! Publishes the host's user-interface settings — text scale, 24-hour clock,
//! light or dark — to `MediaQuery`. Nothing asks for them: the embedder has
//! to send them once at startup and again whenever they change.

use std::collections::HashMap;

use log::debug;
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

/// The name this plugin is registered under.
pub const PLUGIN_NAME: &str = module_path!();
/// The channel this plugin handles.
pub const CHANNEL_NAME: &str = "flutter/settings";

/// Plugin for the `flutter/settings` channel.
#[derive(Default)]
pub struct SettingsPlugin {
    channel: Weak<MessageChannel>,
}

/// One `flutter/settings` message, under construction.
#[must_use = "settings are only delivered by calling .send()"]
pub struct SettingsMessage<'a> {
    plugin: &'a SettingsPlugin,
    settings: HashMap<String, Value>,
}

/// Whether the host asks for a light or a dark interface.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlatformBrightness {
    /// Dark text on a light background.
    Light,
    /// Light text on a dark background.
    Dark,
}

impl PlatformBrightness {
    /// The name the framework's `Brightness` parses.
    fn as_str(&self) -> &'static str {
        // Kept in step with the `rename_all` above by hand, so that building
        // a settings message cannot fail.
        match self {
            PlatformBrightness::Light => "light",
            PlatformBrightness::Dark => "dark",
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
    /// Override the text scale factor.
    pub fn set_text_scale_factor(mut self, factor: f64) -> Self {
        self.settings
            .insert("textScaleFactor".into(), Value::F64(factor));
        self
    }

    /// Override whether times are formatted on a 24-hour clock.
    pub fn set_use_24_hour_format(mut self, use_24_hour_format: bool) -> Self {
        self.settings.insert(
            "alwaysUse24HourFormat".into(),
            Value::Boolean(use_24_hour_format),
        );
        self
    }

    /// Override the platform brightness.
    pub fn set_platform_brightness(mut self, brightness: PlatformBrightness) -> Self {
        self.settings.insert(
            "platformBrightness".into(),
            Value::String(brightness.as_str().to_owned()),
        );
        self
    }

    /// Send the message.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped.
    pub fn send(self) {
        if let Some(channel) = self.plugin.channel.upgrade() {
            debug!("Sending settings: {:?}", self.settings);
            channel.send(self.settings);
        }
    }
}

impl SettingsPlugin {
    /// Start a settings message.
    ///
    /// The framework reads all three of `textScaleFactor`,
    /// `alwaysUse24HourFormat` and `platformBrightness` out of every
    /// `flutter/settings` message and throws on a missing one, so they are
    /// taken up front; the `set_*` methods override them.
    pub fn start_message(
        &self,
        text_scale_factor: f64,
        use_24_hour_format: bool,
        brightness: PlatformBrightness,
    ) -> SettingsMessage<'_> {
        debug!("Starting to build message");
        SettingsMessage {
            plugin: self,
            settings: HashMap::new(),
        }
        .set_text_scale_factor(text_scale_factor)
        .set_use_24_hour_format(use_24_hour_format)
        .set_platform_brightness(brightness)
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
