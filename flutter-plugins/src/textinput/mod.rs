//! Plugin for the `flutter/textinput` channel.
//!
//! `TextField` uses this channel to tell the embedder which field has focus
//! and what it currently contains, and the embedder uses it to report the
//! edits and caret movements it has applied. The model those edits run on is
//! [`TextEditingState`]; an embedder reaches it through
//! [`TextInputPlugin::with_state`] and publishes the result with
//! [`TextInputPlugin::notify_changes`].

use log::{debug, error};
use std::sync::{Arc, Weak};

use serde::{Deserialize, Serialize};

use flutter_engine::codec::value::VecExt;

use flutter_engine::{
    FlutterEngine,
    channel::{MethodCallHandler, MethodChannel},
    codec::JSON_CODEC,
    plugins::Plugin,
};

use flutter_engine::channel::MethodCall;
use flutter_engine::codec::Value;
use parking_lot::{Mutex, RwLock};

pub mod text_editing_state;
pub(crate) mod utils;

pub use self::text_editing_state::TextEditingState;

/// The name this plugin is registered under.
pub const PLUGIN_NAME: &str = module_path!();
/// The channel this plugin handles.
pub const CHANNEL_NAME: &str = "flutter/textinput";

/// The embedder half of the text input protocol: showing and hiding whatever
/// stands in for the on-screen keyboard.
pub trait TextInputHandler {
    /// The focused field wants text input.
    fn show(&mut self);

    /// The focused field no longer wants text input.
    fn hide(&mut self);
}

/// Plugin for the `flutter/textinput` channel.
pub struct TextInputPlugin {
    channel: Weak<MethodChannel>,
    data: Arc<RwLock<Data>>,
    handler: Arc<Mutex<dyn TextInputHandler + Send>>,
}

struct Handler {
    data: Arc<RwLock<Data>>,
    handler: Arc<Mutex<dyn TextInputHandler + Send>>,
}

struct Data {
    client_id: Option<i64>,
    editing_state: Option<TextEditingState>,
}

impl Plugin for TextInputPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        self.channel = engine.register_channel(MethodChannel::new(
            CHANNEL_NAME,
            Handler {
                data: self.data.clone(),
                handler: self.handler.clone(),
            },
            &JSON_CODEC,
        ));
    }
}

impl TextInputPlugin {
    /// A plugin that drives `handler` when the framework asks for text input.
    pub fn new(handler: Arc<Mutex<dyn TextInputHandler + Send>>) -> Self {
        let data = Arc::new(RwLock::new(Data {
            client_id: None,
            editing_state: None,
        }));
        Self {
            channel: Weak::new(),
            handler,
            data,
        }
    }

    fn with_channel<F>(&self, f: F)
    where
        F: FnOnce(&MethodChannel),
    {
        if let Some(channel) = self.channel.upgrade() {
            f(&channel);
        }
    }

    /// Edit the state of the focused field.
    ///
    /// `cbk` is not called at all when no field has focus, i.e. before the
    /// framework's first `TextInput.setEditingState`. Call
    /// [`notify_changes`](Self::notify_changes) afterwards to send the result
    /// back to the framework; nothing else does.
    pub fn with_state(&mut self, cbk: impl FnOnce(&mut TextEditingState)) {
        let mut data = self.data.write();
        if let Some(state) = &mut data.editing_state {
            cbk(state);
        }
    }

    /// Tell the framework that the field's action key — Done, Send, Go, … —
    /// was pressed.
    ///
    /// `action` is the name without its `TextInputAction.` prefix, as given
    /// by the `inputAction` of the `setClient` call.
    pub fn perform_action(&self, action: &str) {
        let data = self.data.read();
        self.with_channel(|channel| {
            let mut args: Vec<Value> = Vec::new();
            if let Err(error) = args.push_as_value(data.client_id) {
                error!("could not encode the text input client id: {error}");
                return;
            }
            if let Err(error) = args.push_as_value("TextInputAction.".to_owned() + action) {
                error!("could not encode the text input action: {error}");
                return;
            }
            channel.invoke_method("TextInputClient.performAction", args)
        });
    }

    /// Send the state of the focused field back to the framework.
    pub fn notify_changes(&mut self) {
        let mut data = self.data.write();
        let client_id = data.client_id;
        if let Some(state) = &mut data.editing_state
            && let Some(channel) = self.channel.upgrade()
        {
            let mut args: Vec<Value> = Vec::new();
            if let Err(error) = args.push_as_value(client_id) {
                error!("could not encode the text input client id: {error}");
                return;
            }
            if let Err(error) = args.push_as_value(state) {
                error!("could not encode the text editing state: {error}");
                return;
            }
            channel.invoke_method("TextInputClient.updateEditingState", args);
        }
    }
}

impl MethodCallHandler for Handler {
    fn on_method_call(&mut self, call: MethodCall) {
        debug!(
            "got method call {} with args {:?}",
            call.method(),
            call.raw_args()
        );
        match call.method() {
            "TextInput.setClient" => {
                let Ok(args) = call.args::<SetClientArgs>() else {
                    return call.error(
                        "invalid-args",
                        "could not deserialize arguments",
                        Value::Null,
                    );
                };
                self.data.write().client_id = Some(args.0);
                call.success_empty()
            }
            "TextInput.clearClient" => {
                let mut data = self.data.write();
                data.client_id = None;
                data.editing_state.take();
                drop(data);
                call.success_empty()
            }
            "TextInput.setEditingState" => {
                let Ok(mut state) = call.args::<TextEditingState>() else {
                    return call.error(
                        "invalid-args",
                        "could not deserialize arguments",
                        Value::Null,
                    );
                };
                // The offsets the framework sends can lag behind the text it
                // sends them with; pin them here so no editing operation has
                // to cope with an offset that is not in the text.
                state.normalize();
                self.data.write().editing_state.replace(state);
                call.success_empty()
            }
            "TextInput.show" => {
                self.handler.lock().show();
                call.success_empty()
            }
            "TextInput.hide" => {
                self.handler.lock().hide();
                call.success_empty()
            }
            _ => call.not_implemented(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SetClientArgs(i64, SetClientArgsText);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetClientArgsText {
    autocorrect: bool,
    input_action: String,
    obscure_text: bool,
    keyboard_appearance: String,
    action_label: Option<String>,
    text_capitalization: String,
    input_type: SetClientArgsInputType,
}

#[derive(Serialize, Deserialize)]
struct SetClientArgsInputType {
    signed: Option<bool>,
    name: String,
    decimal: Option<bool>,
}
