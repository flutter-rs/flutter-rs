//! Plugin for the `flutter/keyboard` channel.
//!
//! `HardwareKeyboard.syncKeyboardState()` calls `getKeyboardState` exactly
//! once, from `ServicesBinding.initInstances`, to learn which keys were
//! already held when the app started. We answer that we do not know, which is
//! correct at startup and is a reply the framework handles. An embedder that
//! can lose keyboard focus mid-press must synthesise the missing
//! `KeyEventType::Up` itself, because nothing re-queries this channel later.

use std::sync::Weak;

use flutter_engine::{
    FlutterEngine,
    channel::{MethodCall, MethodCallHandler, MethodChannel},
    codec::{STANDARD_CODEC, Value},
    plugins::Plugin,
};

/// The name this plugin is registered under.
pub const PLUGIN_NAME: &str = module_path!();
/// The channel this plugin handles.
pub const CHANNEL_NAME: &str = "flutter/keyboard";

/// Plugin for the `flutter/keyboard` channel.
#[derive(Default)]
pub struct KeyboardPlugin {
    channel: Weak<MethodChannel>,
}

impl Plugin for KeyboardPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        self.channel =
            engine.register_channel(MethodChannel::new(CHANNEL_NAME, Handler, &STANDARD_CODEC));
    }
}

struct Handler;

impl MethodCallHandler for Handler {
    fn on_method_call(&mut self, call: MethodCall) {
        match call.method() {
            // The framework wants `Map<int, int>` here (physical key id ->
            // logical key id), which `codec::Value::Map` — keyed by `String`
            // — cannot express; `invokeMapMethod<int, int>` would throw on a
            // stringly-keyed map. Null is read by
            // `HardwareKeyboard.syncKeyboardState` as "no information", which
            // is the honest answer until the codec grows an int-keyed map.
            "getKeyboardState" => call.success(Value::Null),
            _ => call.not_implemented(),
        }
    }
}
