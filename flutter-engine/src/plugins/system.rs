//! Plugin to work with locales.
//! It handles flutter/localization type message.

use super::prelude::*;

use log::{error, info};

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/system";

pub struct SystemPlugin {
    channel: Weak<BasicMessageChannel>,
    handler: Arc<RwLock<Handler>>,
}

impl Default for SystemPlugin {
    fn default() -> Self {
        Self {
            channel: Weak::new(),
            handler: Arc::new(RwLock::new(Handler)),
        }
    }
}

impl Plugin for SystemPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, registrar: &mut ChannelRegistrar) {
        let handler = Arc::downgrade(&self.handler);
        self.channel = registrar.register_channel(BasicMessageChannel::new(
            CHANNEL_NAME,
            handler,
            &json_codec::CODEC,
        ));
    }
}

impl SystemPlugin {
    pub fn send_memory_pressure_warning(&self) {
        info!("Sending memory pressure warning");
        if let Some(channel) = self.channel.upgrade() {
            channel.send(&json_value!({ "type": "memoryPressure"}));
        } else {
            error!("Failed to upgrade channel to send memory pressure warning");
        }
    }
}

struct Handler;

impl MessageHandler for Handler {
    fn on_message(&mut self, _: Value, _: RuntimeData) -> Result<Value, MessageError> {
        Ok(Value::Null)
    }
}
