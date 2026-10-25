//! Plugin for the `flutter-rs/dialog` channel.
//!
//! Opens the host's native file-open and message-box dialogs through
//! `tinyfiledialogs`. Those dialogs are modal and block the thread that opens
//! them until the user answers, so each one runs on a worker thread: blocking
//! the platform thread would stop the engine's platform task runner, and with
//! it the window's painting and its answers to the window manager, for as
//! long as the dialog was open.

use serde::{Deserialize, Serialize};

use flutter_engine::{
    FlutterEngine,
    channel::{MethodCallHandler, MethodChannel},
    codec::{JSON_CODEC, Value},
    plugins::Plugin,
};

use flutter_engine::channel::MethodCall;
use parking_lot::Mutex;

const PLUGIN_NAME: &str = module_path!();
const CHANNEL_NAME: &str = "flutter-rs/dialog";

/// Held for the lifetime of one dialog, so that two concurrent calls queue up
/// rather than racing two native dialogs onto the screen. The platform thread
/// used to serialise them by being blocked by them.
static DIALOG_LOCK: Mutex<()> = Mutex::new(());

/// Plugin for the `flutter-rs/dialog` channel.
#[derive(Default)]
pub struct DialogPlugin {}

struct Handler;

impl Plugin for DialogPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        engine.register_channel(MethodChannel::new(CHANNEL_NAME, Handler, &JSON_CODEC));
    }
}

impl MethodCallHandler for Handler {
    fn on_method_call(&mut self, call: MethodCall) {
        match call.method() {
            "open_file_dialog" => {
                let OpenFileDialogParams {
                    title,
                    path,
                    filter,
                } = match call.args::<OpenFileDialogParams>() {
                    Ok(args) => args,
                    Err(_) => {
                        return call.error(
                            "invalid-args",
                            "could not deserialize arguments",
                            Value::Null,
                        );
                    }
                };
                if title.iter().chain(path.iter()).any(|s| s.contains('\0'))
                    || filter.as_ref().is_some_and(|(patterns, name)| {
                        name.contains('\0') || patterns.iter().any(|p| p.contains('\0'))
                    })
                {
                    return call.error("invalid-args", NUL_IN_ARGUMENTS, Value::Null);
                }

                // `MethodCall::respond` hops back to the platform thread on
                // its own, so the worker can answer Dart directly.
                std::thread::spawn(move || {
                    let filter = filter.as_ref().map(|(patterns, name)| {
                        let patterns: Vec<&str> = patterns.iter().map(String::as_str).collect();
                        (patterns, name.as_str())
                    });
                    let _dialog = DIALOG_LOCK.lock();
                    let chosen = tinyfiledialogs::open_file_dialog(
                        title.as_deref().unwrap_or(""),
                        path.as_deref().unwrap_or(""),
                        filter
                            .as_ref()
                            .map(|(patterns, name)| (patterns.as_slice(), *name)),
                    );
                    call.success(chosen.as_deref().unwrap_or(""));
                });
            }
            "message_box_ok" => {
                let MessageBoxOkParams {
                    title,
                    message,
                    icon,
                } = match call.args::<MessageBoxOkParams>() {
                    Ok(args) => args,
                    Err(_) => {
                        return call.error(
                            "invalid-args",
                            "could not deserialize arguments",
                            Value::Null,
                        );
                    }
                };
                if title.contains('\0') || message.contains('\0') {
                    return call.error("invalid-args", NUL_IN_ARGUMENTS, Value::Null);
                }

                let icon = match icon.unwrap_or(MessageBoxIcon::Info) {
                    MessageBoxIcon::Info => tinyfiledialogs::MessageBoxIcon::Info,
                    MessageBoxIcon::Error => tinyfiledialogs::MessageBoxIcon::Error,
                    MessageBoxIcon::Question => tinyfiledialogs::MessageBoxIcon::Question,
                    MessageBoxIcon::Warning => tinyfiledialogs::MessageBoxIcon::Warning,
                };
                std::thread::spawn(move || {
                    let _dialog = DIALOG_LOCK.lock();
                    tinyfiledialogs::message_box_ok(title.as_str(), message.as_str(), icon);
                    call.success_empty();
                });
            }
            _ => call.not_implemented(),
        }
    }
}

/// `tinyfiledialogs` builds `CString`s from the strings it is given and
/// panics on an interior NUL byte, so they are rejected before they get
/// there: a panicking worker thread would strand the awaiting Dart future.
const NUL_IN_ARGUMENTS: &str = "arguments must not contain a NUL byte";

#[derive(Serialize, Deserialize)]
struct OpenFileDialogParams {
    title: Option<String>,
    path: Option<String>,
    filter: Option<(Vec<String>, String)>,
}

#[derive(Serialize, Deserialize)]
struct MessageBoxOkParams {
    title: String,
    message: String,
    icon: Option<MessageBoxIcon>,
}

/// Which icon a message box shows.
#[derive(Serialize, Deserialize)]
pub enum MessageBoxIcon {
    /// Informational.
    Info,
    /// A warning.
    Warning,
    /// An error.
    Error,
    /// A question.
    Question,
}
