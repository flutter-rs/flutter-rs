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
///
/// Deliberately *not* `#[non_exhaustive]`: `dart:ui`'s `Brightness` has these
/// two values and nothing else, and `platformBrightness` is parsed by a `switch`
/// that falls back to light.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlatformBrightness {
    /// Dark text on a light background. The framework's own default, and the
    /// value `_updateUserSettingsData` falls back to for anything it cannot
    /// parse.
    #[default]
    Light,
    /// Light text on a dark background.
    Dark,
}

impl PlatformBrightness {
    /// The name the framework's `Brightness` parses.
    #[must_use]
    pub fn as_str(self) -> &'static str {
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

    /// The settings this message will send, for inspection in tests.
    #[cfg(test)]
    fn settings(&self) -> &HashMap<String, Value> {
        &self.settings
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
    /// taken up front; the `set_*` methods override them. The optional keys
    /// `_updateUserSettingsData` also looks for — `brieflyShowPassword`,
    /// `nativeSpellCheckServiceDefined`, `supportsShowingSystemContextMenu` —
    /// are left out, which the framework reads as their defaults.
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

#[cfg(test)]
mod tests {
    use super::{PlatformBrightness, SettingsPlugin};
    use flutter_engine::codec::Value;

    /// `_updateUserSettingsData` parses `platformBrightness` with a `switch`
    /// on these two literal strings and silently falls back to light for
    /// anything else, so a typo here is an invisible bug.
    #[test]
    fn brightness_names_match_the_frameworks_parser() {
        assert_eq!(PlatformBrightness::Light.as_str(), "light");
        assert_eq!(PlatformBrightness::Dark.as_str(), "dark");
    }

    /// The framework's default, and what it falls back to for a value it
    /// cannot parse.
    #[test]
    fn brightness_defaults_to_light() {
        assert_eq!(PlatformBrightness::default(), PlatformBrightness::Light);
    }

    /// `textScaleFactor`, `alwaysUse24HourFormat` and `platformBrightness` are
    /// all dereferenced with `!` on the Dart side: a message missing one of
    /// them throws inside `dart:ui` rather than being ignored.
    #[test]
    fn a_started_message_carries_every_key_the_framework_requires() {
        let plugin = SettingsPlugin::default();
        let message = plugin.start_message(1.5, true, PlatformBrightness::Dark);
        let settings = message.settings();

        assert_eq!(settings.get("textScaleFactor"), Some(&Value::F64(1.5)));
        assert_eq!(
            settings.get("alwaysUse24HourFormat"),
            Some(&Value::Boolean(true))
        );
        assert_eq!(
            settings.get("platformBrightness"),
            Some(&Value::String("dark".to_owned()))
        );
        assert_eq!(
            settings.len(),
            3,
            "an unexpected key was added: {settings:?}"
        );
    }
}
