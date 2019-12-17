//! Plugin to work with locales.
//! It handles flutter/localization type message.

use log::debug;

use super::prelude::*;

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/lifecycle";

pub struct LifecyclePlugin {
    channel: Weak<BasicMessageChannel>,
    handler: Arc<RwLock<Handler>>,
}

impl Default for LifecyclePlugin {
    fn default() -> Self {
        Self {
            channel: Weak::new(),
            handler: Arc::new(RwLock::new(Handler)),
        }
    }
}

impl Plugin for LifecyclePlugin {
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

impl LifecyclePlugin {
    pub fn send_app_is_inactive(&self) {
        debug!("Sending app is inactive");
        if let Some(channel) = self.channel.upgrade() {
            channel.send(&Value::String("AppLifecycleState.inactive".to_owned()));
        }
    }

    pub fn send_app_is_resumed(&self) {
        debug!("Sending app is resumed");
        if let Some(channel) = self.channel.upgrade() {
            channel.send(&Value::String("AppLifecycleState.resumed".to_owned()));
        }
    }

    pub fn send_app_is_paused(&self) {
        debug!("Sending app is paused");
        if let Some(channel) = self.channel.upgrade() {
            channel.send(&Value::String("AppLifecycleState.paused".to_owned()));
        }
    }
}

struct Handler;

impl MessageHandler for Handler {
    fn on_message(&mut self, _: Value, _: FlutterEngine) -> Result<Value, MessageError> {
        Ok(Value::Null)
    }
}
