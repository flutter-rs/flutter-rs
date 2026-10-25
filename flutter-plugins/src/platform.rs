//! Plugin for the `flutter/platform` channel.
//!
//! Carries the clipboard, the window-chrome and the app-exit calls the
//! framework makes on the host: `Clipboard.getData`/`setData`/`hasStrings`,
//! `SystemChrome.setApplicationSwitcherDescription`,
//! `SystemNavigator.pop` and `System.exitApplication`. Anything else is
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

/// Why the framework is asking the embedder to exit, mirroring
/// `ui.AppExitType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AppExitType {
    /// The app may refuse: an `onExitRequested` observer can cancel it.
    Cancelable,
    /// The app has already decided; the embedder is only being told.
    Required,
}

/// The embedder's answer to [`PlatformHandler::exit_application`], mirroring
/// `ui.AppExitResponse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AppExitResponse {
    /// The embedder is closing the app.
    Exit,
    /// The embedder is not closing; the framework keeps running.
    Cancel,
}

impl AppExitResponse {
    /// The wire value `ServicesBinding.exitApplication` matches on.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Exit => "exit",
            Self::Cancel => "cancel",
        }
    }
}

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

    /// Close the app, or refuse to.
    ///
    /// The framework asks for this through `SystemNavigator.pop`, which is
    /// always [`AppExitType::Required`] and ignores the answer, and through
    /// `ServicesBinding.exitApplication`, where an app that installed an
    /// `onExitRequested` observer may ask for a [`AppExitType::Cancelable`]
    /// exit. Answering [`AppExitResponse::Cancel`] to a `Required` request
    /// leaves the user with a window that cannot be closed, because the
    /// framework has already decided by then.
    ///
    /// `exit_code` is the status the app would like the process to end with;
    /// an embedder that cannot choose its own exit status should say so in its
    /// implementation rather than pretend.
    fn exit_application(&mut self, exit_type: AppExitType, exit_code: i32) -> AppExitResponse;
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
            // The older, unconditional form: the framework has decided
            // already and discards the reply. Its `animated` argument is an
            // Android transition hint with no desktop meaning.
            "SystemNavigator.pop" => {
                self.handler
                    .lock()
                    .exit_application(AppExitType::Required, 0);
                call.success_empty()
            }
            // The negotiated form. `ServicesBinding.exitApplication` reads the
            // reply as `result['response']` and treats a null result as
            // `cancel`, so the answer has to be this exact map.
            "System.exitApplication" => {
                let (exit_type, exit_code) = exit_request(call.raw_args());
                let response = self.handler.lock().exit_application(exit_type, exit_code);
                call.success(ExitApplicationResponse {
                    response: response.as_str(),
                })
            }
            _ => call.not_implemented(),
        }
    }
}

/// Read `System.exitApplication`'s arguments, which the framework sends as
/// `{"type": "cancelable" | "required", "exitCode": int}`.
///
/// Anything unreadable counts as `required`: the embedder cannot tell whether
/// the app is in a state to be asked, and refusing an exit the framework
/// believes it already performed is the worse failure of the two.
fn exit_request(args: &Value) -> (AppExitType, i32) {
    let Value::Map(args) = args else {
        return (AppExitType::Required, 0);
    };

    let exit_type = match args.get("type") {
        Some(Value::String(name)) if name == "cancelable" => AppExitType::Cancelable,
        _ => AppExitType::Required,
    };
    let exit_code = match args.get("exitCode") {
        Some(Value::I32(code)) => *code,
        // The JSON codec this channel uses widens every int it reads.
        Some(Value::I64(code)) => i32::try_from(*code).unwrap_or_default(),
        _ => 0,
    };

    (exit_type, exit_code)
}

/// The reply to `System.exitApplication`.
#[derive(Debug, Serialize)]
struct ExitApplicationResponse {
    response: &'static str,
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    fn map(entries: &[(&str, Value)]) -> Value {
        Value::Map(
            entries
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect::<HashMap<_, _>>(),
        )
    }

    #[test]
    fn the_frameworks_exit_arguments_are_read_as_sent() {
        // `ServicesBinding.exitApplication` sends `AppExitType.name` and an
        // int, and the JSON codec widens the int.
        assert_eq!(
            exit_request(&map(&[
                ("type", Value::String("cancelable".to_owned())),
                ("exitCode", Value::I64(0)),
            ])),
            (AppExitType::Cancelable, 0)
        );
        assert_eq!(
            exit_request(&map(&[
                ("type", Value::String("required".to_owned())),
                ("exitCode", Value::I32(2)),
            ])),
            (AppExitType::Required, 2)
        );
    }

    #[test]
    fn an_unreadable_exit_request_is_treated_as_required() {
        // Refusing an exit the framework believes it has already performed
        // would leave a window the user cannot close, so every unreadable
        // shape has to fall this way.
        assert_eq!(exit_request(&Value::Null), (AppExitType::Required, 0));
        assert_eq!(exit_request(&map(&[])), (AppExitType::Required, 0));
        assert_eq!(
            exit_request(&map(&[("type", Value::I64(1))])),
            (AppExitType::Required, 0)
        );
        assert_eq!(
            exit_request(&map(&[
                ("type", Value::String("Cancelable".to_owned())),
                ("exitCode", Value::String("0".to_owned())),
            ])),
            (AppExitType::Required, 0)
        );
    }

    #[test]
    fn an_out_of_range_exit_code_does_not_wrap() {
        assert_eq!(
            exit_request(&map(&[("exitCode", Value::I64(i64::from(i32::MAX) + 1))])),
            (AppExitType::Required, 0)
        );
    }

    #[test]
    fn the_response_strings_match_the_frameworks_parser() {
        // `ServicesBinding.exitApplication` matches on these two literals and
        // treats everything else as `exit`.
        assert_eq!(AppExitResponse::Exit.as_str(), "exit");
        assert_eq!(AppExitResponse::Cancel.as_str(), "cancel");
    }
}
