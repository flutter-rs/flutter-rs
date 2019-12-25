//! Plugin to work with clipboard and various system related functions.
//! It handles flutter/platform type message.
use crate::prelude::*;
use log::debug;
use parking_lot::Mutex;

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/platform";

#[derive(Debug)]
pub struct MimeError;

impl std::fmt::Display for MimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Mime error")
    }
}

impl std::error::Error for MimeError {}

pub trait PlatformHandler {
    fn set_application_switcher_description(&mut self, description: AppSwitcherDescription);

    fn set_clipboard_data(&mut self, text: String);

    fn get_clipboard_data(&mut self, mime: &str) -> Result<String, MimeError>;
}

pub struct PlatformPlugin {
    channel: Weak<JsonMethodChannel>,
    handler: Arc<RwLock<Handler>>,
}

impl PlatformPlugin {
    pub fn new(handler: Arc<Mutex<Box<dyn PlatformHandler + Send>>>) -> Self {
        Self {
            channel: Weak::new(),
            handler: Arc::new(RwLock::new(Handler { handler })),
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

struct Handler {
    handler: Arc<Mutex<Box<dyn PlatformHandler + Send>>>,
}

impl MethodCallHandler for Handler {
    fn on_method_call(
        &mut self,
        call: MethodCall,
        _: FlutterEngine,
    ) -> Result<Value, MethodCallError> {
        debug!("got method call {} with args {:?}", call.method, call.args);
        match call.method.as_str() {
            "SystemChrome.setApplicationSwitcherDescription" => {
                let args: AppSwitcherDescription = from_value(&call.args)?;
                self.handler
                    .lock()
                    .set_application_switcher_description(args);
                Ok(Value::Null)
            }
            "Clipboard.setData" => {
                if let Value::Map(v) = &call.args {
                    if let Some(v) = &v.get("text") {
                        if let Value::String(text) = v {
                            let text = text.clone();
                            self.handler.lock().set_clipboard_data(text);
                            return Ok(Value::Null);
                        }
                    }
                }
                Err(MethodCallError::UnspecifiedError)
            }
            "Clipboard.getData" => {
                if let Value::String(mime) = &call.args {
                    match self.handler.lock().get_clipboard_data(mime) {
                        Ok(text) => Ok(json_value!({ "text": text })),
                        Err(_) => Err(MethodCallError::UnspecifiedError),
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
#[serde(rename_all = "camelCase")]
pub struct AppSwitcherDescription {
    pub primary_color: i64,
    pub label: String,
}
