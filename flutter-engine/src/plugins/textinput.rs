//! This plugin is used by TextField to edit text and control caret movement.
//! It handles flutter/textinput type message.

mod text_editing_state;

use self::text_editing_state::TextEditingState;
use crate::{
    channel::{Channel, JsonMethodChannel},
    codec::MethodCall,
    codec::MethodCallResult,
    desktop_window_state::RuntimeData,
    plugins::{PlatformMessage, Plugin, PluginChannel},
    utils::{OwnedStringUtils, StringUtils},
};

use std::{cell::RefCell, sync::Weak};

use glfw::Modifiers;
use log::{error, warn};
use serde_json::{json, Value};

pub const CHANNEL_NAME: &str = "flutter/textinput";

pub struct TextInputPlugin {
    client_id: Option<i64>,
    editing_state: RefCell<Option<TextEditingState>>,
    channel: JsonMethodChannel,
}

impl PluginChannel for TextInputPlugin {
    fn channel_name() -> &'static str {
        CHANNEL_NAME
    }
}

impl TextInputPlugin {
    pub fn new() -> TextInputPlugin {
        TextInputPlugin {
            client_id: None,
            editing_state: RefCell::new(None),
            channel: JsonMethodChannel::new(CHANNEL_NAME),
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
        self.channel.invoke_method(MethodCall {
            method: String::from("TextInputClient.performAction"),
            args: json!([self.client_id, "TextInputAction.".to_owned() + action]),
        });
    }

    pub fn notify_changes(&self) {
        self.with_state(|s: &mut TextEditingState| {
            self.channel.invoke_method(MethodCall {
                method: String::from("TextInputClient.updateEditingState"),
                args: json!([self.client_id, s]),
            });
        });
    }
}

impl Plugin for TextInputPlugin {
    fn init_channel(&mut self, runtime_data: Weak<RuntimeData>) {
        self.channel.init(runtime_data);
    }
    fn handle(&mut self, msg: &PlatformMessage, _: &mut glfw::Window) {
        let decoded = self.channel.decode_method_call(msg).unwrap();

        match decoded.method.as_str() {
            "TextInput.setClient" => {
                if let Value::Array(v) = &decoded.args {
                    if v.len() > 0 {
                        if let Some(n) = v[0].as_i64() {
                            self.client_id = Some(n);
                        }
                    }
                }
            }
            "TextInput.clearClient" => {
                self.client_id = None;
                self.editing_state.replace(None);
            }
            "TextInput.setEditingState" => {
                if self.client_id.is_some() {
                    self.editing_state
                        .replace(TextEditingState::from(&decoded.args));
                }
            }
            "TextInput.show" => {}
            "TextInput.hide" => {}
            method => {
                warn!("Unknown method {} called", method);
            }
        }
    }
}
