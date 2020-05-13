//! Plugin to handle system dialogs.
//! It handles flutter-rs/dialog type message.

use serde::{Deserialize, Serialize};

use flutter_engine::{
    channel::{MethodCallHandler, MethodChannel},
    codec::JSON_CODEC,
    plugins::Plugin,
    FlutterEngine,
};

use flutter_engine::channel::MethodCall;

const PLUGIN_NAME: &str = module_path!();
const CHANNEL_NAME: &str = "flutter-rs/dialog";

pub struct DialogPlugin {}

struct Handler;

impl Default for DialogPlugin {
    fn default() -> Self {
        Self {}
    }
}

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
        match call.method().as_str() {
            "open_file_dialog" => {
                let OpenFileDialogParams {
                    title,
                    path,
                    filter,
                } = call.args::<OpenFileDialogParams>();

                // Oh, these borrow stuff sux
                let filter2 = filter.as_ref().map(|(p, n)| {
                    let p: Vec<&str> = p.iter().map(String::as_str).collect();
                    (p, n)
                });
                let path = tinyfiledialogs::open_file_dialog(
                    title.as_ref().unwrap_or(&String::from("")),
                    path.as_ref().unwrap_or(&String::from("")),
                    filter2.as_ref().map(|(p, n)| (p.as_slice(), n.as_str())),
                );

                call.success(match &path {
                    Some(p) => p,
                    None => "",
                })
            }
            "message_box_ok" => {
                let MessageBoxOkParams {
                    title,
                    message,
                    icon,
                } = call.args::<MessageBoxOkParams>();

                let icon = match icon.unwrap_or(MessageBoxIcon::Info) {
                    MessageBoxIcon::Info => tinyfiledialogs::MessageBoxIcon::Info,
                    MessageBoxIcon::Error => tinyfiledialogs::MessageBoxIcon::Error,
                    MessageBoxIcon::Question => tinyfiledialogs::MessageBoxIcon::Question,
                    MessageBoxIcon::Warning => tinyfiledialogs::MessageBoxIcon::Warning,
                };
                tinyfiledialogs::message_box_ok(title.as_str(), message.as_str(), icon);
                call.success_empty()
            }
            _ => call.not_implemented(),
        }
    }
}

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

#[derive(Serialize, Deserialize)]
pub enum MessageBoxIcon {
    Info,
    Warning,
    Error,
    Question,
}
