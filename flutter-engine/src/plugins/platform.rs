//! Plugin to work with clipboard and various system related functions.
//! It handles flutter/platform type message.

use super::prelude::*;

use log::error;

pub const PLUGIN_NAME: &str = "flutter-engine::plugins::platform";
pub const CHANNEL_NAME: &str = "flutter/platform";

#[derive(Default)]
pub struct PlatformPlugin {
    channel: Weak<JsonMethodChannel>,
}

impl PlatformPlugin {
    pub fn new() -> Self {
        Self {
            channel: Weak::new(),
        }
    }
}

impl Plugin for PlatformPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, plugin: Weak<RwLock<Self>>, registrar: &mut ChannelRegistrar) {
        self.channel = registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, plugin));
    }
}

impl MethodCallHandler for PlatformPlugin {
    fn on_method_call(
        &mut self,
        channel: &str,
        call: MethodCall,
        window: &mut Window,
    ) -> Result<Value, MethodCallError> {
        match call.method.as_str() {
            "SystemChrome.setApplicationSwitcherDescription" => {
                let args: SetApplicationSwitcherDescriptionArgs = from_value(&call.args)?;
                // label and primaryColor
                window.set_title(args.label.as_str());
                Ok(Value::Null)
            }
            "Clipboard.setData" => {
                if let Value::Map(v) = &call.args {
                    if let Some(v) = &v.get("text") {
                        if let Value::String(text) = v {
                            window.set_clipboard_string(text);
                            return Ok(Value::Null);
                        }
                    }
                }
                Err(MethodCallError::UnspecifiedError)
            }
            "Clipboard.getData" => {
                if let Value::String(mime) = &call.args {
                    match mime.as_str() {
                        "text/plain" => Ok(json_value!({
                            "text": window.get_clipboard_string()
                        })),
                        _ => {
                            error!(
                                "Don't know how to handle {} clipboard message",
                                mime.as_str()
                            );
                            Err(MethodCallError::UnspecifiedError)
                        }
                    }
                } else {
                    Err(MethodCallError::UnspecifiedError)
                }
            }
            method => Err(MethodCallError::NotImplemented),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SetApplicationSwitcherDescriptionArgs {
    pub label: String,
}
