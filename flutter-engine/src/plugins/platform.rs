//! Plugin to work with clipboard and various system related functions.
//! It handles flutter/platform type message.

use super::prelude::*;

use log::{debug, error};

pub const PLUGIN_NAME: &str = "flutter-engine::plugins::platform";
pub const CHANNEL_NAME: &str = "flutter/platform";

pub struct PlatformPlugin {
    channel: Weak<JsonMethodChannel>,
    handler: Arc<RwLock<Handler>>,
}

impl Default for PlatformPlugin {
    fn default() -> Self {
        Self {
            channel: Weak::new(),
            handler: Arc::new(RwLock::new(Handler)),
        }
    }
}

impl Plugin for PlatformPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, registrar: &mut ChannelRegistrar) {
        let method_handler = Arc::downgrade(&self.handler);
        self.channel =
            registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, method_handler));
    }
}

struct Handler;

impl MethodCallHandler for Handler {
    fn on_method_call(
        &mut self,
        call: MethodCall,
        runtime_data: RuntimeData,
    ) -> Result<Value, MethodCallError> {
        debug!("got method call {} with args {:?}", call.method, call.args);
        match call.method.as_str() {
            "SystemChrome.setApplicationSwitcherDescription" => {
                let args: SetApplicationSwitcherDescriptionArgs = from_value(&call.args)?;
                // label and primaryColor
                runtime_data.with_window(move |window| {
                    window.set_title(args.label.as_str());
                })?;
                Ok(Value::Null)
            }
            "Clipboard.setData" => {
                if let Value::Map(v) = &call.args {
                    if let Some(v) = &v.get("text") {
                        if let Value::String(text) = v {
                            let text = text.clone();
                            runtime_data.with_window(move |window| {
                                window.set_clipboard_string(text.as_str());
                            })?;
                            return Ok(Value::Null);
                        }
                    }
                }
                Err(MethodCallError::UnspecifiedError)
            }
            "Clipboard.getData" => {
                if let Value::String(mime) = &call.args {
                    match mime.as_str() {
                        "text/plain" => {
                            let text = runtime_data
                                .with_window_result(|window| window.get_clipboard_string())?;
                            Ok(json_value!({ "text": text }))
                        }
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
            _ => Err(MethodCallError::NotImplemented),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SetApplicationSwitcherDescriptionArgs {
    pub label: String,
}
