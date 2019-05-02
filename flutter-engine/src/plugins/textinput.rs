//! This plugin is used by TextField to edit text and control caret movement.
//! It handles flutter/textinput type message.

mod text_editing_state;

use self::text_editing_state::TextEditingState;
use super::prelude::*;

pub const PLUGIN_NAME: &str = "flutter-engine::plugins::textinput";
pub const CHANNEL_NAME: &str = "flutter/textinput";

#[derive(Default)]
pub struct TextInputPlugin {
    client_id: Option<i64>,
    editing_state: Option<TextEditingState>,
    channel: Weak<JsonMethodChannel>,
}

impl Plugin for TextInputPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, plugin: Weak<RwLock<Self>>, registrar: &mut ChannelRegistrar) {
        self.channel = registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, plugin));
    }
}

impl TextInputPlugin {
    pub fn new() -> Self {
        Self {
            client_id: None,
            editing_state: None,
            channel: Weak::new(),
        }
    }

    fn with_channel<F>(&self, f: F)
    where
        F: FnOnce(&Channel),
    {
        if let Some(channel) = self.channel.upgrade() {
            f(&*channel);
        }
    }

    pub fn with_state(&mut self, cbk: impl FnOnce(&mut TextEditingState)) {
        if let Some(state) = &mut self.editing_state {
            cbk(state);
        }
    }

    pub fn perform_action(&self, action: &str) {
        self.with_channel(|channel| {
            channel.invoke_method(MethodCall {
                method: String::from("TextInputClient.performAction"),
                args: json_value!([self.client_id, "TextInputAction.".to_owned() + action]),
            })
        });
    }

    pub fn notify_changes(&mut self) {
        if let Some(state) = &mut self.editing_state {
            if let Some(channel) = self.channel.upgrade() {
                channel.invoke_method(MethodCall {
                    method: String::from("TextInputClient.updateEditingState"),
                    args: json_value!([self.client_id, state]),
                });
            }
        };
    }
}

impl MethodCallHandler for TextInputPlugin {
    fn on_method_call(
        &mut self,
        channel: &str,
        call: MethodCall,
        _: &mut Window,
    ) -> Result<Value, MethodCallError> {
        match call.method.as_str() {
            "TextInput.setClient" => {
                if let Value::List(v) = &call.args {
                    if !v.is_empty() {
                        if let Value::I64(n) = v[0] {
                            self.client_id = Some(n);
                            return Ok(Value::Null);
                        }
                    }
                }
                Err(MethodCallError::UnspecifiedError)
            }
            "TextInput.clearClient" => {
                self.client_id = None;
                self.editing_state.take();
                Ok(Value::Null)
            }
            "TextInput.setEditingState" => {
                if self.client_id.is_some() {
                    let state: TextEditingState = from_value(&call.args)?;
                    self.editing_state.replace(state);
                    Ok(Value::Null)
                } else {
                    Err(MethodCallError::UnspecifiedError)
                }
            }
            "TextInput.show" => Ok(Value::Null),
            "TextInput.hide" => Ok(Value::Null),
            _ => Err(MethodCallError::NotImplemented),
        }
    }
}
