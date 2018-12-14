use crate::{FlutterEngineInner};
use super::{Plugin, PlatformMessage};
use serde_json::Value;

#[derive(Default)]
pub struct PlatformPlugin {}

impl Plugin for PlatformPlugin {
    fn get_channel(&self) -> String {
        String::from("flutter/platform")
    }
    fn handle(&mut self, msg: &PlatformMessage, engine: &FlutterEngineInner, window: &mut glfw::Window) {
        match msg.message.method.as_str() {
            "SystemChrome.setApplicationSwitcherDescription" => {
                // label and primaryColor
                window.set_title(msg.message.args.as_object().unwrap().get("label").unwrap().as_str().unwrap());
            },
            "Clipboard.setData" => (
                if let Value::Object(v) = &msg.message.args {
                    if let Some(v) = &v.get("text") {
                        if let Value::String(text) = v {
                            window.set_clipboard_string(text);
                        }
                    }
                }
            ),
            "Clipboard.getData" => (
                if let Value::String(mime) = &msg.message.args {
                    match mime.as_str() {
                        "text/plain" => {
                            let json = json!([{
                                "text": window.get_clipboard_string(),
                            }]);
                            engine.send_platform_message_response(&msg, json.to_string().as_bytes());
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
