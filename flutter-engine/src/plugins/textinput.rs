//! This plugin is used by TextField to edit text and control caret movement.
//! It handles flutter/textinput type message.

mod text_editing_state;

use self::text_editing_state::TextEditingState;
use super::prelude::*;

use std::cell::RefCell;

pub const PLUGIN_NAME: &str = "flutter-engine::plugins::textinput";
pub const CHANNEL_NAME: &str = "flutter/textinput";

#[derive(Default)]
pub struct TextInputPlugin {
    client_id: Option<i64>,
    editing_state: RefCell<Option<TextEditingState>>,
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
            editing_state: RefCell::new(None),
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

    pub fn with_state(&self, cbk: impl Fn(&mut TextEditingState)) {
        if let Ok(mut state) = self.editing_state.try_borrow_mut() {
            if let Some(state) = &mut *state {
                cbk(state);
            }
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

    pub fn notify_changes(&self) {
        self.with_state(|s: &mut TextEditingState| {
            self.with_channel(|channel| {
                channel.invoke_method(MethodCall {
                    method: String::from("TextInputClient.updateEditingState"),
                    args: json_value!([self.client_id, s]),
                });
            })
        });
    }
}

impl MethodCallHandler for TextInputPlugin {
    fn on_method_call(
        &mut self,
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
                self.editing_state.replace(None);
                Ok(Value::Null)
            }
            "TextInput.setEditingState" => {
                if self.client_id.is_some() {
                    self.editing_state
                        .replace(TextEditingState::from(call.args));
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
