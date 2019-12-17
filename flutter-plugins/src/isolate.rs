//! Plugin to work with locales.
//! It handles flutter/localization type message.

use super::prelude::*;
use parking_lot::Mutex;

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/isolate";

pub type IsolateCallbackFn = Mutex<Option<Box<dyn FnOnce() + Send>>>;

pub struct IsolatePlugin {
    channel: Weak<BasicMessageChannel>,
    handler: Arc<RwLock<Handler>>,
}

impl IsolatePlugin {
    pub fn new<F>(callback: F) -> Self
    where
        F: FnOnce() -> () + 'static + Send,
    {
        Self {
            channel: Weak::new(),
            handler: Arc::new(RwLock::new(Handler {
                callback: Mutex::new(Some(Box::new(callback))),
            })),
        }
    }
}

impl Plugin for IsolatePlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, registrar: &mut ChannelRegistrar) {
        let handler = Arc::downgrade(&self.handler);
        self.channel = registrar.register_channel(BasicMessageChannel::new(
            CHANNEL_NAME,
            handler,
            &string_codec::CODEC,
        ));
    }
}

struct Handler {
    callback: IsolateCallbackFn,
}

impl MessageHandler for Handler {
    fn on_message(&mut self, _: Value, _: FlutterEngine) -> Result<Value, MessageError> {
        if let Some(callback) = self.callback.lock().take() {
            (callback)();
        }
        Ok(Value::Null)
    }
}
