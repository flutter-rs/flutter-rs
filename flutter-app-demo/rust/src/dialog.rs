//! Plugin to handle system dialogs.
//! It handles flutter-rs/dialog type message.

use flutter_engine::plugins::prelude::*;

const PLUGIN_NAME: &str = module_path!();
const CHANNEL_NAME: &str = "flutter-rs/dialog";

pub struct DialogPlugin {
    channel: Weak<JsonMethodChannel>,
}

impl DialogPlugin {
    pub fn new() -> Self {
        DialogPlugin {
            channel: Weak::new(),
        }
    }
}

impl Plugin for DialogPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, plugin: Weak<RwLock<Self>>, registrar: &mut ChannelRegistrar) {
        self.channel = registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, plugin));
    }
}

impl MethodCallHandler for DialogPlugin {
    fn handle_async(&self, _call: &MethodCall) -> bool {
        // all calls here should be handled async
        true
    }

    fn on_method_call(
        &mut self,
        _channel: &str,
        _call: MethodCall,
        _window: &mut Window,
    ) -> Result<Value, MethodCallError> {
        // this can't happen as all calls are handled async
        Err(MethodCallError::UnspecifiedError)
    }

    fn on_async_method_call(
        &mut self,
        _channel: &str,
        call: MethodCall,
        window: &mut Window,
        mut response_handle: Option<PlatformMessageResponseHandle>,
    ) {
        match call.method.as_str() {
            "open_file_dialog" => {
                let channel = self.channel.clone();
                let mut response_handle = response_handle.take();
                std::thread::spawn(move || {
                    let params = from_value::<OpenFileDialogParams>(&call.args);
                    if params.is_err() {
                        let channel = channel.upgrade().unwrap();
                        channel.send_method_call_response(
                            &mut response_handle,
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
                    let channel = channel.upgrade().unwrap();
                    channel.send_method_call_response(
                        &mut response_handle,
                        MethodCallResult::Ok(json_value!(s)),
                    );
                });
            }
            "message_box_ok" => {
                let channel = self.channel.clone();
                let mut response_handle = response_handle.take();
                std::thread::spawn(move || {
                    let params = from_value::<MessageBoxOkParams>(&call.args);
                    if params.is_err() {
                        let channel = channel.upgrade().unwrap();
                        channel.send_method_call_response(
                            &mut response_handle,
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

                    let channel = channel.upgrade().unwrap();
                    channel.send_method_call_response(
                        &mut response_handle,
                        MethodCallResult::Ok(Value::Null),
                    );
                });
            }
            _ => self
                .channel
                .upgrade()
                .unwrap()
                .send_method_call_response(&mut response_handle, MethodCallResult::NotImplemented),
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
