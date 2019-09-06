//! This plugin is used by TextField to edit text and control caret movement.
//! It handles flutter/textinput type message.

mod text_editing_state;

use self::text_editing_state::TextEditingState;
use crate::prelude::*;

use log::debug;

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/textinput";

pub struct TextInputPlugin {
    channel: Weak<JsonMethodChannel>,
    handler: Arc<RwLock<Handler>>,
    data: Arc<RwLock<Data>>,
}

struct Handler {
    data: Arc<RwLock<Data>>,
}

struct Data {
    client_id: Option<i64>,
    editing_state: Option<TextEditingState>,
}

impl Plugin for TextInputPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, registrar: &mut ChannelRegistrar) {
        let method_handler = Arc::downgrade(&self.handler);
        self.channel =
            registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, method_handler));
    }
}

impl Default for TextInputPlugin {
    fn default() -> Self {
        let data = Arc::new(RwLock::new(Data {
            client_id: None,
            editing_state: None,
        }));
        Self {
            channel: Weak::new(),
            handler: Arc::new(RwLock::new(Handler {
                data: Arc::clone(&data),
            })),
            data,
        }
    }
}

impl TextInputPlugin {
    fn with_channel<F>(&self, f: F)
    where
        F: FnOnce(&Channel),
    {
        if let Some(channel) = self.channel.upgrade() {
            f(&*channel);
        }
    }

    pub fn with_state(&mut self, cbk: impl FnOnce(&mut TextEditingState)) {
        let mut data = self.data.write().unwrap();
        if let Some(state) = &mut data.editing_state {
            cbk(state);
        }
    }

    pub fn perform_action(&self, action: &str) {
        let data = self.data.read().unwrap();
        self.with_channel(|channel| {
            channel.invoke_method(MethodCall {
                method: String::from("TextInputClient.performAction"),
                args: json_value!([data.client_id, "TextInputAction.".to_owned() + action]),
            })
        });
    }

    pub fn notify_changes(&mut self) {
        let mut data = self.data.write().unwrap();
        let client_id = data.client_id;
        if let Some(state) = &mut (data.editing_state) {
            if let Some(channel) = self.channel.upgrade() {
                channel.invoke_method(MethodCall {
                    method: String::from("TextInputClient.updateEditingState"),
                    args: json_value!([client_id, state]),
                });
            }
        };
    }
}

impl MethodCallHandler for Handler {
    fn on_method_call(
        &mut self,
        call: MethodCall,
        _: RuntimeData,
    ) -> Result<Value, MethodCallError> {
        debug!("got method call {} with args {:?}", call.method, call.args);
        match call.method.as_str() {
            "TextInput.setClient" => {
                let mut data = self.data.write().unwrap();
                let args: SetClientArgs = from_value(&call.args)?;
                data.client_id = Some(args.0);
                Ok(Value::Null)
            }
            "TextInput.clearClient" => {
                let mut data = self.data.write().unwrap();
                data.client_id = None;
                data.editing_state.take();
                Ok(Value::Null)
            }
            "TextInput.setEditingState" => {
                let mut data = self.data.write().unwrap();
                let state: TextEditingState = from_value(&call.args)?;
                data.editing_state.replace(state);
                Ok(Value::Null)
            }
            "TextInput.show" => Ok(Value::Null),
            "TextInput.hide" => Ok(Value::Null),
            _ => Err(MethodCallError::NotImplemented),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SetClientArgs(i64, SetClientArgsText);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetClientArgsText {
    autocorrect: bool,
    input_action: String,
    obscure_text: bool,
    keyboard_appearance: String,
    action_label: Option<String>,
    text_capitalization: String,
    input_type: SetClientArgsInputType,
}

#[derive(Serialize, Deserialize)]
struct SetClientArgsInputType {
    signed: Option<bool>,
    name: String,
    decimal: Option<bool>,
}
