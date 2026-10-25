//! Plugin for the `flutter/mousecursor` channel.
//! The framework asks the platform to change the system mouse cursor as the
//! pointer moves over widgets that declare a `MouseCursor`.
use std::sync::{Arc, Weak};

use flutter_engine::{
    FlutterEngine,
    channel::{MethodCall, MethodCallHandler, MethodChannel},
    codec::{STANDARD_CODEC, Value},
    plugins::Plugin,
};
use log::{debug, warn};
use parking_lot::Mutex;

/// The name this plugin is registered under.
pub const PLUGIN_NAME: &str = module_path!();
/// The channel this plugin handles.
pub const CHANNEL_NAME: &str = "flutter/mousecursor";

/// The embedder half of the `flutter/mousecursor` channel.
pub trait MouseCursorHandler {
    /// Activate the system cursor identified by `kind`, one of the kinds
    /// defined by Flutter's `SystemMouseCursors` (e.g. `basic`, `click`,
    /// `text`, `none`, `resizeLeftRight`, ...).
    fn activate_system_cursor(&mut self, kind: &str);
}

/// Plugin for the `flutter/mousecursor` channel.
pub struct MouseCursorPlugin {
    channel: Weak<MethodChannel>,
    handler: Arc<Mutex<dyn MouseCursorHandler + Send>>,
}

impl MouseCursorPlugin {
    /// A plugin that serves the channel out of `handler`.
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
        match call.method() {
            "activateSystemCursor" => {
                if let Value::Map(args) = call.raw_args()
                    && let Some(Value::String(kind)) = args.get("kind")
                {
                    debug!("activateSystemCursor: {kind}");
                    self.handler.lock().activate_system_cursor(kind);
                    return call.success(true);
                }
                // `false` tells the framework the cursor was not applied, so
                // that it can fall back to its own handling.
                warn!("activateSystemCursor without a kind: {:?}", call.raw_args());
                call.success(false)
            }
            _ => call.not_implemented(),
        }
    }
}
