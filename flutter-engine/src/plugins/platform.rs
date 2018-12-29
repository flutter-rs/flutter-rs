use crate::{FlutterEngineInner};
use super::{Plugin, PlatformMessage, PluginRegistry};
use serde_json::Value;
use channel::{ Channel, JsonMethodChannel };
use codec::MethodCallResult;

pub struct PlatformPlugin {
    channel: JsonMethodChannel,
}

impl PlatformPlugin {
    pub fn new() -> Self {
        PlatformPlugin {
            channel: JsonMethodChannel::new("flutter/platform")
        }
    }
}

impl Plugin for PlatformPlugin {
    fn init_channel(&self, registry: &PluginRegistry) -> &str {
        self.channel.init(registry);
        return self.channel.get_name();
    }
    fn handle(&mut self, msg: &PlatformMessage, _engine: &FlutterEngineInner, window: &mut glfw::Window) {
        let decoded = self.channel.decode_method_call(msg);
        match decoded.method.as_str() {
            "SystemChrome.setApplicationSwitcherDescription" => {
                // label and primaryColor
                window.set_title(decoded.args.as_object().unwrap().get("label").unwrap().as_str().unwrap());
            },
            "Clipboard.setData" => {
                if let Value::Object(v) = &decoded.args {
                    if let Some(v) = &v.get("text") {
                        if let Value::String(text) = v {
                            window.set_clipboard_string(text);
                        }
                    }
                }
            },
            "Clipboard.getData" => (
                if let Value::String(mime) = &decoded.args {
                    match mime.as_str() {
                        "text/plain" => {
                            self.channel.send_method_call_response(
                                msg.response_handle,
                                MethodCallResult::Ok(json!({
                                    "text": window.get_clipboard_string(),
                                })),
                            );
                        },
                        _ => {
                            error!("Dont know how to handle {} clipboard message", mime.as_str());
                            ()
                        },
                    }
                }
            ),
            _ => (),
        }
    }
}
