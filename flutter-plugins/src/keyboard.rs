//! Plugin for the `flutter/keyboard` channel.
//! `HardwareKeyboard` calls `getKeyboardState` to synchronise the set of
//! physically pressed keys (e.g. after a focus change). We report an empty
//! state, which is correct whenever no keys are held at query time; subsequent
//! key events keep the framework in sync from there.
use std::collections::HashMap;
use std::sync::Weak;

use flutter_engine::{
    channel::{MethodCall, MethodCallHandler, MethodChannel},
    codec::{Value, STANDARD_CODEC},
    plugins::Plugin,
    FlutterEngine,
};

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/keyboard";

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
        match call.method().as_str() {
            "getKeyboardState" => call.success(Value::Map(HashMap::new())),
            _ => call.not_implemented(),
        }
    }
}
