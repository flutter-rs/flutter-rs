//! Plugin to work with locales.
//! It handles flutter/localization type message.

use super::prelude::*;

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/isolate";

pub struct IsolatePlugin {
    channel: Weak<BasicMessageChannel>,
    handler: Arc<RwLock<Handler>>,
}

impl Default for IsolatePlugin {
    fn default() -> Self {
        Self {
            channel: Weak::new(),
            handler: Arc::new(RwLock::new(Handler)),
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

struct Handler;

impl MessageHandler for Handler {
    fn on_message(&mut self, _: Value, runtime_data: RuntimeData) -> Result<Value, MessageError> {
        runtime_data.with_window_state(|window_state| {
            window_state.set_isolate_created();
        })?;
        Ok(Value::Null)
    }
}
