//! Plugin for the `flutter/keyevent` channel, Flutter's legacy `RawKeyboard`
//! protocol.
//!
//! The framework builds `RawKeyEvent` from these messages, and reads the
//! `character` of one from a `unicodeScalarValues` field that [`KeyAction`]
//! has never carried. The protocol itself is gone from current Flutter in
//! favour of `FlutterKeyEvent`, which the embedder sends through
//! [`FlutterEngine::send_key_event`](flutter_engine::FlutterEngine::send_key_event)
//! rather than over a channel, so nothing in tree registers this plugin any
//! more.

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
pub const CHANNEL_NAME: &str = "flutter/keyevent";

/// Plugin for the `flutter/keyevent` channel.
#[derive(Default)]
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

/// One legacy raw key event.
#[derive(Serialize, Deserialize)]
pub struct KeyAction {
    /// Name of the toolkit the event came from, e.g. `"glfw"`.
    pub toolkit: String,
    /// Toolkit-specific key code.
    #[serde(rename = "keyCode")]
    pub key_code: i32,
    /// Toolkit-specific scan code.
    #[serde(rename = "scanCode")]
    pub scan_code: i32,
    /// Toolkit-specific modifier bitmask.
    pub modifiers: i32,
    /// Name of the keymap the framework should parse this with, e.g.
    /// `"linux"`.
    pub keymap: String,
    /// Whether the key went down or up.
    #[serde(rename = "type")]
    pub _type: KeyActionType,
}

/// Whether a [`KeyAction`] is a press or a release.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyActionType {
    /// The key went down.
    Keydown,
    /// The key came up.
    Keyup,
}

impl KeyEventPlugin {
    fn with_channel<F>(&self, f: F)
    where
        F: FnOnce(&MessageChannel),
    {
        if let Some(channel) = self.channel.upgrade() {
            f(&channel);
        }
    }

    /// Push one raw key event to the framework.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped.
    pub fn key_action(&self, action: KeyAction) {
        self.with_channel(|channel| {
            channel.send(action);
        });
    }
}

impl MessageHandler for Handler {
    fn on_message(&mut self, msg: Message) {
        // The framework does not send on this channel, but a reply it is not
        // waiting for would only be logged as an error.
        if msg.can_respond() {
            msg.respond(Value::Null);
        }
    }
}
