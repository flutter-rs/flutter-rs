//! Plugin to work with locales.
//! It handles flutter/localization type message.

use super::prelude::*;

use log::{debug, error, info};

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/system";

pub struct SystemPlugin {
    channel: Weak<JsonMethodChannel>,
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
        let method_handler = Arc::downgrade(&self.handler);
        self.channel =
            registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, method_handler));
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

impl MethodCallHandler for Handler {
    fn on_method_call(
        &mut self,
        call: MethodCall,
        _runtime_data: RuntimeData,
    ) -> Result<Value, MethodCallError> {
        debug!("got method call {} with args {:?}", call.method, call.args);
        Err(MethodCallError::NotImplemented)
    }
}
