//! Plugin to work with locales.
//! It handles flutter/localization type message.
use std::sync::{Arc, Weak};

use flutter_engine::{
    channel::{MessageChannel, MessageHandler},
    codec::STRING_CODEC,
    plugins::Plugin,
    FlutterEngine,
};

use flutter_engine::channel::Message;
use flutter_engine::codec::Value;
use parking_lot::Mutex;

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/isolate";

pub type IsolateCallbackFn = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

pub struct IsolatePlugin {
    channel: Weak<MessageChannel>,
    callback: IsolateCallbackFn,
}

impl IsolatePlugin {
    pub fn new<F>(callback: F) -> Self
    where
        F: FnOnce() -> () + 'static + Send,
    {
        Self {
            channel: Weak::new(),
            callback: Arc::new(Mutex::new(Some(Box::new(callback)))),
        }
    }

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
        msg.respond(Value::Null)
    }
}
