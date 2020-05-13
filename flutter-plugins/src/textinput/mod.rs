//! This plugin is used by TextField to edit text and control caret movement.
//! It handles flutter/textinput type message.

use log::debug;
use std::sync::{Arc, RwLock, Weak};

use serde::{Deserialize, Serialize};

use flutter_engine::codec::value::VecExt;

use flutter_engine::{
    channel::{MethodCallHandler, MethodChannel},
    codec::JSON_CODEC,
    plugins::Plugin,
    FlutterEngine,
};

use self::text_editing_state::TextEditingState;
use flutter_engine::channel::MethodCall;
use flutter_engine::codec::Value;
use parking_lot::Mutex;

mod text_editing_state;
pub(crate) mod utils;

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/textinput";

pub trait TextInputHandler {
    fn show(&mut self);

    fn hide(&mut self);
}

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
            f(&*channel);
        }
    }

    pub fn with_state(&mut self, cbk: impl FnOnce(&mut TextEditingState)) {
        let mut data = self.data.write().unwrap();
        if let Some(state) = &mut data.editing_state {
            cbk(state);
        }
    }

    pub fn perform_action(&self, action: &str) {
        let data = self.data.read().unwrap();
        self.with_channel(|channel| {
            let mut args: Vec<Value> = Vec::new();
            args.push_as_value(data.client_id);
            args.push_as_value("TextInputAction.".to_owned() + action);
            channel.invoke_method("TextInputClient.performAction", args)
        });
    }

    pub fn notify_changes(&mut self) {
        let mut data = self.data.write().unwrap();
        let client_id = data.client_id;
        if let Some(state) = &mut (data.editing_state) {
            if let Some(channel) = self.channel.upgrade() {
                let mut args: Vec<Value> = Vec::new();
                args.push_as_value(client_id);
                args.push_as_value(state);
                channel.invoke_method("TextInputClient.updateEditingState", args)
            }
        };
    }
}

impl MethodCallHandler for Handler {
    fn on_method_call(&mut self, call: MethodCall) {
        debug!(
            "got method call {} with args {:?}",
            call.method(),
            call.raw_args()
        );
        match call.method().as_str() {
            "TextInput.setClient" => {
                let mut data = self.data.write().unwrap();
                let args: SetClientArgs = call.args();
                data.client_id = Some(args.0);
                call.success_empty()
            }
            "TextInput.clearClient" => {
                let mut data = self.data.write().unwrap();
                data.client_id = None;
                data.editing_state.take();
                call.success_empty()
            }
            "TextInput.setEditingState" => {
                let mut data = self.data.write().unwrap();
                let state: TextEditingState = call.args();
                data.editing_state.replace(state);
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
