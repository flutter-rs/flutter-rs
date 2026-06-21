//! Plugin for the `flutter/mousecursor` channel.
//! The framework asks the platform to change the system mouse cursor as the
//! pointer moves over widgets that declare a `MouseCursor`.
use std::sync::{Arc, Weak};

use flutter_engine::{
    channel::{MethodCall, MethodCallHandler, MethodChannel},
    codec::{Value, STANDARD_CODEC},
    plugins::Plugin,
    FlutterEngine,
};
use log::debug;
use parking_lot::Mutex;

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/mousecursor";

pub trait MouseCursorHandler {
    /// Activate the system cursor identified by `kind`, one of the kinds
    /// defined by Flutter's `SystemMouseCursors` (e.g. `basic`, `click`,
    /// `text`, `none`, `resizeLeftRight`, ...).
    fn activate_system_cursor(&mut self, kind: &str);
}

pub struct MouseCursorPlugin {
    channel: Weak<MethodChannel>,
    handler: Arc<Mutex<dyn MouseCursorHandler + Send>>,
}

impl MouseCursorPlugin {
    pub fn new(handler: Arc<Mutex<dyn MouseCursorHandler + Send>>) -> Self {
        Self {
            channel: Weak::new(),
            handler,
        }
    }
}

impl Plugin for MouseCursorPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        self.channel = engine.register_channel(MethodChannel::new(
            CHANNEL_NAME,
            Handler {
                handler: self.handler.clone(),
            },
            &STANDARD_CODEC,
        ));
    }
}

struct Handler {
    handler: Arc<Mutex<dyn MouseCursorHandler + Send>>,
}

impl MethodCallHandler for Handler {
    fn on_method_call(&mut self, call: MethodCall) {
        match call.method().as_str() {
            "activateSystemCursor" => {
                if let Value::Map(args) = call.raw_args() {
                    if let Some(Value::String(kind)) = args.get("kind") {
                        debug!("activateSystemCursor: {}", kind);
                        self.handler.lock().activate_system_cursor(kind);
                        return call.success(true);
                    }
                }
                call.success(false)
            }
            _ => call.not_implemented(),
        }
    }
}
