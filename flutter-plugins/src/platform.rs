//! Plugin to work with clipboard and various system related functions.
//! It handles flutter/platform type message.
use std::sync::{Arc, Weak};

use flutter_engine::{
    channel::{MethodCallHandler, MethodChannel},
    codec::JSON_CODEC,
    plugins::Plugin,
    FlutterEngine,
};

use serde::{Deserialize, Serialize};

use flutter_engine::channel::MethodCall;
use flutter_engine::codec::Value;
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
    channel: Weak<MethodChannel>,
    handler: Arc<Mutex<dyn PlatformHandler + Send>>,
}

impl PlatformPlugin {
    pub fn new(handler: Arc<Mutex<dyn PlatformHandler + Send>>) -> Self {
        Self {
            channel: Weak::new(),
            handler,
        }
    }
}

impl Plugin for PlatformPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        self.channel = engine.register_channel(MethodChannel::new(
            CHANNEL_NAME,
            Handler {
                handler: self.handler.clone(),
            },
            &JSON_CODEC,
        ));
    }
}

struct Handler {
    handler: Arc<Mutex<dyn PlatformHandler + Send>>,
}

impl MethodCallHandler for Handler {
    fn on_method_call(&mut self, call: MethodCall) {
        debug!(
            "got method call {} with args {:?}",
            call.method(),
            call.raw_args()
        );
        match call.method().as_str() {
            "SystemChrome.setApplicationSwitcherDescription" => {
                let args: AppSwitcherDescription = call.args();
                self.handler
                    .lock()
                    .set_application_switcher_description(args);
                call.success_empty()
            }
            "Clipboard.setData" => {
                if let Value::Map(v) = &call.args() {
                    if let Some(v) = &v.get("text") {
                        if let Value::String(text) = v {
                            let text = text.clone();
                            self.handler.lock().set_clipboard_data(text);
                            return call.success_empty();
                        }
                    }
                }
                call.error("unknown-data", "Unknown data type", Value::Null)
            }
            "Clipboard.getData" => {
                if let Value::String(mime) = call.raw_args() {
                    match self.handler.lock().get_clipboard_data(&mime) {
                        Ok(text) => call.success(ClipboardData { text }),
                        Err(_) => call.error("unknown-data", "Unknown data type", Value::Null),
                    }
                } else {
                    call.error("unknown-data", "Unknown data type", Value::Null)
                }
            }
            _ => call.not_implemented(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ClipboardData {
    text: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSwitcherDescription {
    pub primary_color: i64,
    pub label: String,
}
