use super::prelude::*;

use glfw;

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/keyevent";

pub struct KeyEventPlugin {
    channel: Weak<BasicMessageChannel>,
    handler: Arc<RwLock<Handler>>,
}

struct Handler;

impl Plugin for KeyEventPlugin {
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

impl Default for KeyEventPlugin {
    fn default() -> Self {
        Self {
            channel: Weak::new(),
            handler: Arc::new(RwLock::new(Handler)),
        }
    }
}

impl KeyEventPlugin {
    fn with_channel<F>(&self, f: F)
    where
        F: FnOnce(&BasicMessageChannel),
    {
        if let Some(channel) = self.channel.upgrade() {
            f(&*channel);
        }
    }

    pub fn key_action(&self, down: bool, key: glfw::Key, scancode: glfw::Scancode, modifiers: glfw::Modifiers) {
        self.with_channel(|channel| {
            let json = json_value!({
                "toolkit": "glfw",
                "keyCode": key as i32,
                "scanCode": scancode as i32,
                // "codePoint": 
                "modifiers": modifiers.bits() as i32,
                // TODO: raw_keyboard_listener.dart seems to have limited support for keyboard
                // need to update later
                "keymap": "linux",
                "type": if down { "keyup" } else { "keydown" }
            });
            channel.send(&json);
        });
    }
}

impl MessageHandler for Handler {
    fn on_message(&mut self, _: Value, _: RuntimeData) -> Result<Value, MessageError> {
        Ok(Value::Null)
    }
}
