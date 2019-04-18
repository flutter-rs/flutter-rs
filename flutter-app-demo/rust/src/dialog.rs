//! Plugin to handle system dialogs.
//! It handles flutter-rs/dialog type message.

use std::sync::{Arc, Mutex, Weak};

use flutter_engine::{
    channel::{Channel, JsonMethodChannel},
    codec::{json_codec::Value, MethodCallResult},
    plugins::{Plugin, PluginChannel},
    PlatformMessage, RuntimeData, Window,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

const CHANNEL_NAME: &str = "flutter-rs/dialog";

pub struct DialogPlugin {
    channel: Arc<Mutex<JsonMethodChannel>>,
    // handle: Handle,
}

impl DialogPlugin {
    pub fn new() -> Self {
        DialogPlugin {
            channel: Arc::new(Mutex::new(JsonMethodChannel::new(CHANNEL_NAME))),
        }
    }
}

impl PluginChannel for DialogPlugin {
    fn channel_name() -> &'static str {
        CHANNEL_NAME
    }
}

impl Plugin for DialogPlugin {
    fn init_channel(&mut self, registry: Weak<RuntimeData>) {
        let mut channel = self.channel.lock().unwrap();
        channel.init(registry);
    }

    fn handle(&mut self, msg: &PlatformMessage, _window: &mut Window) {
        let channel = self.channel.lock().unwrap();
        let decoded = channel.decode_method_call(msg).unwrap();
        match decoded.method.as_str() {
            "open_file_dialog" => {
                let c = self.channel.clone();
                let handle = msg.response_handle.unwrap();
                std::thread::spawn(move || {
                    let s = serde_json::to_string(&decoded.args);
                    let params: serde_json::Result<OpenFileDialogParams> =
                        serde_json::from_str(&s.unwrap());
                    if params.is_err() {
                        let channel = c.lock().unwrap();
                        channel.send_method_call_response(
                            handle,
                            MethodCallResult::Err {
                                code: "1002".to_owned(), // TODO: put errors together
                                message: "Params error".to_owned(),
                                details: Value::Null,
                            },
                        );
                        return;
                    }
                    let params = params.unwrap();
                    let OpenFileDialogParams {
                        title,
                        path,
                        filter,
                    } = params;

                    // Oh, these borrow stuff sux
                    let filter2 = filter.as_ref().map(|(p, n)| {
                        let p: Vec<&str> = p.iter().map(|v| v.as_str()).collect();
                        (p, n)
                    });
                    let path = tinyfiledialogs::open_file_dialog(
                        title.as_ref().unwrap_or(&String::from("")),
                        path.as_ref().unwrap_or(&String::from("")),
                        filter2.as_ref().map(|(p, n)| (p.as_slice(), n.as_str())),
                    );

                    let s = match &path {
                        Some(p) => p,
                        None => "",
                    };
                    let channel = c.lock().unwrap();
                    channel.send_method_call_response(handle, MethodCallResult::Ok(json!(s)));
                });
            }
            "message_box_ok" => {
                let c = self.channel.clone();
                let handle = msg.response_handle.unwrap();
                std::thread::spawn(move || {
                    let s = serde_json::to_string(&decoded.args);
                    let params: serde_json::Result<MessageBoxOkParams> =
                        serde_json::from_str(&s.unwrap());
                    if params.is_err() {
                        let channel = c.lock().unwrap();
                        channel.send_method_call_response(
                            handle,
                            MethodCallResult::Err {
                                code: "1002".to_owned(), // TODO: put errors together
                                message: "Params error".to_owned(),
                                details: Value::Null,
                            },
                        );
                        return;
                    }
                    let params = params.unwrap();
                    let MessageBoxOkParams {
                        title,
                        message,
                        icon,
                    } = params;

                    let icon = match icon.unwrap_or(MessageBoxIcon::Info) {
                        MessageBoxIcon::Info => tinyfiledialogs::MessageBoxIcon::Info,
                        MessageBoxIcon::Error => tinyfiledialogs::MessageBoxIcon::Error,
                        MessageBoxIcon::Question => tinyfiledialogs::MessageBoxIcon::Question,
                        MessageBoxIcon::Warning => tinyfiledialogs::MessageBoxIcon::Warning,
                    };
                    tinyfiledialogs::message_box_ok(title.as_str(), message.as_str(), icon);

                    let channel = c.lock().unwrap();
                    channel.send_method_call_response(
                        handle,
                        MethodCallResult::Ok(json!(Value::Null)),
                    );
                });
            }
            _ => (),
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
