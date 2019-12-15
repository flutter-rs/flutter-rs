//! Plugin to handle system dialogs.
//! It handles flutter-rs/dialog type message.

use super::prelude::*;

const PLUGIN_NAME: &str = module_path!();
const CHANNEL_NAME: &str = "flutter-rs/dialog";

pub struct DialogPlugin {
    handler: Arc<RwLock<Handler>>,
}

struct Handler;

impl Default for DialogPlugin {
    fn default() -> Self {
        Self {
            handler: Arc::new(RwLock::new(Handler)),
        }
    }
}

impl Plugin for DialogPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, registrar: &mut ChannelRegistrar) {
        let method_handler = Arc::downgrade(&self.handler);
        registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, method_handler));
    }
}

impl MethodCallHandler for Handler {
    fn on_method_call(
        &mut self,
        call: MethodCall,
        _: FlutterEngine,
    ) -> Result<Value, MethodCallError> {
        match call.method.as_str() {
            "open_file_dialog" => {
                let params = from_value::<OpenFileDialogParams>(&call.args)?;
                let OpenFileDialogParams {
                    title,
                    path,
                    filter,
                } = params;

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

                let s = match &path {
                    Some(p) => p,
                    None => "",
                };
                Ok(json_value!(s))
            }
            "message_box_ok" => {
                let params = from_value::<MessageBoxOkParams>(&call.args)?;
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
                Ok(Value::Null)
            }
            _ => Err(MethodCallError::NotImplemented),
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
