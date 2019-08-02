use super::prelude::*;

use glfw;
use serde_json::json;

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/keyevent";

pub struct KeyEventPlugin {
    channel: Weak<JsonMethodChannel>,
    handler: Arc<RwLock<Handler>>,
}

struct Handler;

impl Plugin for KeyEventPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, registrar: &mut ChannelRegistrar) {
        let method_handler = Arc::downgrade(&self.handler);
        self.channel =
            registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, method_handler));
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
        F: FnOnce(&Channel),
    {
        if let Some(channel) = self.channel.upgrade() {
            f(&*channel);
        }
    }

    pub fn key_action(&self, down: bool, key: glfw::Key, scancode: glfw::Scancode, modifiers: glfw::Modifiers) {
        self.with_channel(|channel| {
            let json = json!({
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
            let s = serde_json::to_string(&json).unwrap();
            channel.send_buffer(&s.into_bytes());
        });
    }
}

impl MethodCallHandler for Handler {
    fn on_method_call(
        &mut self,
        _: MethodCall,
        _: RuntimeData,
    ) -> Result<Value, MethodCallError> {
        Ok(Value::Null)
    }
}
