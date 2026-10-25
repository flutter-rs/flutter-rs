//! Plugin for the `flutter/platform` channel.
//!
//! Carries the clipboard and the window-chrome calls the framework makes on
//! the host: `Clipboard.getData`/`setData`/`hasStrings` and
//! `SystemChrome.setApplicationSwitcherDescription`. Anything else is
//! answered as not implemented, which is what `SystemChannels.platform` — an
//! `OptionalMethodChannel` — expects for a call the embedder does not
//! support.
use std::sync::{Arc, Weak};

use flutter_engine::{
    FlutterEngine,
    channel::{MethodCallHandler, MethodChannel},
    codec::JSON_CODEC,
    plugins::Plugin,
};

use serde::{Deserialize, Serialize};

use flutter_engine::channel::MethodCall;
use flutter_engine::codec::Value;
use log::debug;
use parking_lot::Mutex;

/// The name this plugin is registered under.
pub const PLUGIN_NAME: &str = module_path!();
/// The channel this plugin handles.
pub const CHANNEL_NAME: &str = "flutter/platform";

/// The clipboard does not hold the requested mime type.
#[derive(Debug, thiserror::Error)]
#[error("unsupported clipboard mime type")]
pub struct MimeError;

/// The embedder half of the `flutter/platform` channel.
pub trait PlatformHandler {
    /// Describe the app to the host's window/app switcher.
    fn set_application_switcher_description(&mut self, description: AppSwitcherDescription);

    /// Put `text` on the host clipboard.
    fn set_clipboard_data(&mut self, text: String);

    /// Read the host clipboard.
    ///
    /// # Errors
    ///
    /// Fails with [`MimeError`] if the clipboard cannot produce `mime`. The
    /// framework only ever asks for `text/plain`.
    fn get_clipboard_data(&mut self, mime: &str) -> Result<String, MimeError>;
}

/// Plugin for the `flutter/platform` channel.
pub struct PlatformPlugin {
    channel: Weak<MethodChannel>,
    handler: Arc<Mutex<dyn PlatformHandler + Send>>,
}

impl PlatformPlugin {
    /// A plugin that serves the channel out of `handler`.
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
        match call.method() {
            "SystemChrome.setApplicationSwitcherDescription" => {
                let Ok(args) = call.args::<AppSwitcherDescription>() else {
                    return call.error(
                        "invalid-args",
                        "could not deserialize arguments",
                        Value::Null,
                    );
                };
                self.handler
                    .lock()
                    .set_application_switcher_description(args);
                call.success_empty()
            }
            "Clipboard.setData" => {
                if let Value::Map(args) = call.raw_args()
                    && let Some(Value::String(text)) = args.get("text")
                {
                    self.handler.lock().set_clipboard_data(text.clone());
                    return call.success_empty();
                }
                call.error("unknown-data", "Unknown data type", Value::Null)
            }
            "Clipboard.getData" => {
                if let Value::String(mime) = call.raw_args() {
                    match self.handler.lock().get_clipboard_data(mime) {
                        Ok(text) => call.success(ClipboardData { text }),
                        Err(error) => call.error("unknown-data", error.to_string(), Value::Null),
                    }
                } else {
                    call.error("unknown-data", "Unknown data type", Value::Null)
                }
            }
            "Clipboard.hasStrings" => {
                // `Clipboard.hasStrings` is what the text selection toolbar
                // polls to decide whether to offer Paste, and it reads
                // `result['value'] as bool`.
                let has_strings = self
                    .handler
                    .lock()
                    .get_clipboard_data(CLIPBOARD_TEXT_PLAIN)
                    .is_ok_and(|text| !text.is_empty());
                call.success(HasStringsResult { value: has_strings })
            }
            _ => call.not_implemented(),
        }
    }
}

/// `Clipboard.kTextPlain`, the only mime type the framework asks for.
const CLIPBOARD_TEXT_PLAIN: &str = "text/plain";

#[derive(Serialize, Deserialize)]
struct ClipboardData {
    text: String,
}

#[derive(Serialize, Deserialize)]
struct HasStringsResult {
    value: bool,
}

/// How the app describes itself to the host's app switcher.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSwitcherDescription {
    /// ARGB color of the app's switcher entry.
    pub primary_color: i64,
    /// Label of the app's switcher entry.
    pub label: String,
}
