use super::prelude::*;

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

#[derive(Serialize, Deserialize)]
pub struct KeyAction {
    pub toolkit: String,
    #[serde(rename = "keyCode")]
    pub key_code: i32,
    #[serde(rename = "scanCode")]
    pub scan_code: i32,
    pub modifiers: i32,
    pub keymap: String,
    #[serde(rename = "type")]
    pub _type: KeyActionType,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyActionType {
    Keydown,
    Keyup,
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

    pub fn key_action(&self, action: KeyAction) {
        self.with_channel(|channel| {
            let json = json_value!(action);
            channel.send(&json);
        });
    }
}

impl MessageHandler for Handler {
    fn on_message(&mut self, _: Value, _: FlutterEngine) -> Result<Value, MessageError> {
        Ok(Value::Null)
    }
}
