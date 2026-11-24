//! Plugin for the `flutter/isolate` channel.
//!
//! The root isolate sends one message here once it has started, which the
//! embedder uses to run a startup callback — the point where the engine is
//! live enough to be sent messages of its own.
use std::sync::{Arc, Weak};

use flutter_engine::{
    FlutterEngine,
    channel::{MessageChannel, MessageHandler},
    codec::STRING_CODEC,
    plugins::Plugin,
};

use flutter_engine::channel::Message;
use flutter_engine::codec::Value;
use parking_lot::Mutex;

/// The name this plugin is registered under.
pub const PLUGIN_NAME: &str = module_path!();
/// The channel this plugin handles.
pub const CHANNEL_NAME: &str = "flutter/isolate";

/// The startup callback, in the shape the plugin shares it in.
pub type IsolateCallbackFn = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

/// Plugin for the `flutter/isolate` channel.
pub struct IsolatePlugin {
    channel: Weak<MessageChannel>,
    callback: IsolateCallbackFn,
}

impl IsolatePlugin {
    /// A plugin that runs `callback` once, when the root isolate starts.
    ///
    /// `callback` runs on the platform thread, inside the engine's message
    /// dispatch.
    pub fn new<F>(callback: F) -> Self
    where
        F: FnOnce() + 'static + Send,
    {
        Self {
            channel: Weak::new(),
            callback: Arc::new(Mutex::new(Some(Box::new(callback)))),
        }
    }

    /// A plugin that answers the channel and does nothing else.
    pub fn new_stub() -> Self {
        Self {
            channel: Weak::new(),
            callback: Arc::new(Mutex::new(None)),
        }
    }
}

impl Plugin for IsolatePlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        self.channel = engine.register_channel(MessageChannel::new(
            CHANNEL_NAME,
            Handler {
                callback: self.callback.clone(),
            },
            &STRING_CODEC,
        ));
    }
}

struct Handler {
    callback: IsolateCallbackFn,
}

impl MessageHandler for Handler {
    fn on_message(&mut self, msg: Message) {
        if let Some(callback) = self.callback.lock().take() {
            (callback)();
        }
        if msg.can_respond() {
            msg.respond(Value::Null);
        }
    }
}
