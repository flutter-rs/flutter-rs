//! Plugin to work with clipboard and various system related functions.
//! It handles flutter/platform type message.

use super::{PlatformMessage, Plugin, PluginName};
use crate::{
    channel::{Channel, JsonMethodChannel},
    codec::MethodCallResult,
    desktop_window_state::RuntimeData,
};

use std::sync::Weak;

use log::{error, warn};
use serde_json::{json, Value};

pub const PLUGIN_NAME: &str = "flutter-engine::plugins::platform";
pub const CHANNEL_NAME: &str = "flutter/platform";

pub struct PlatformPlugin {
    channel: JsonMethodChannel,
}

impl PlatformPlugin {
    pub fn new() -> Self {
        Self {
            channel: JsonMethodChannel::new(CHANNEL_NAME),
        }
    }
}

impl PluginName for PlatformPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }
}

impl Plugin for PlatformPlugin {
    fn init_channel(&mut self, runtime_data: Weak<RuntimeData>) {
        self.channel.init(runtime_data);
    }

    fn handle(&mut self, msg: &mut PlatformMessage, window: &mut glfw::Window) {
        let decoded = self.channel.decode_method_call(msg).unwrap();
        match decoded.method.as_str() {
            "SystemChrome.setApplicationSwitcherDescription" => {
                // label and primaryColor
                window.set_title(
                    decoded
                        .args
                        .as_object()
                        .unwrap()
                        .get("label")
                        .unwrap()
                        .as_str()
                        .unwrap(),
                );
                self.channel.send_method_call_response(
                    &mut msg.response_handle,
                    MethodCallResult::Ok(Value::Null),
                );
            }
            "Clipboard.setData" => {
                if let Value::Object(v) = &decoded.args {
                    if let Some(v) = &v.get("text") {
                        if let Value::String(text) = v {
                            window.set_clipboard_string(text);
                            self.channel.send_method_call_response(
                                &mut msg.response_handle,
                                MethodCallResult::Ok(Value::Null),
                            );
                        }
                    }
                }
                self.channel.send_method_call_response(
                    &mut msg.response_handle,
                    MethodCallResult::Err {
                        message: "".into(),
                        code: "".into(),
                        details: Value::Null,
                    },
                );
            }
            "Clipboard.getData" => {
                if let Value::String(mime) = &decoded.args {
                    match mime.as_str() {
                        "text/plain" => self.channel.send_method_call_response(
                            &mut msg.response_handle,
                            MethodCallResult::Ok(json!({
                                "text": window.get_clipboard_string(),
                            })),
                        ),
                        _ => {
                            error!(
                                "Don't know how to handle {} clipboard message",
                                mime.as_str()
                            );
                            self.channel.send_method_call_response(
                                &mut msg.response_handle,
                                MethodCallResult::Err {
                                    message: "".into(),
                                    code: "".into(),
                                    details: Value::Null,
                                },
                            );
                        }
                    }
                }
            }
            method => {
                warn!("Unknown method {} called", method);
                self.channel.send_method_call_response(
                    &mut msg.response_handle,
                    MethodCallResult::NotImplemented,
                );
            }
        }
    }
}
