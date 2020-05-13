//! Plugin to handle system dialogs.
//! It handles flutter-rs/dialog type message.
use std::sync::{Arc, Weak};

use serde::{Deserialize, Serialize};

use flutter_engine::{
    channel::{MethodCallHandler, MethodChannel},
    codec::JSON_CODEC,
    plugins::Plugin,
    FlutterEngine,
};

use flutter_engine::channel::MethodCall;
use parking_lot::Mutex;

const PLUGIN_NAME: &str = module_path!();
const CHANNEL_NAME: &str = "flutter-rs/window";

pub trait WindowHandler {
    fn close(&mut self);

    fn show(&mut self);

    fn hide(&mut self);

    fn maximize(&mut self);

    fn iconify(&mut self);

    fn restore(&mut self);

    fn is_maximized(&mut self) -> bool;

    fn is_iconified(&mut self) -> bool;

    fn is_visible(&mut self) -> bool;

    fn set_pos(&mut self, pos: PositionParams);

    fn get_pos(&mut self) -> PositionParams;

    fn start_drag(&mut self);

    fn end_drag(&mut self);
}

pub struct WindowPlugin {
    channel: Weak<MethodChannel>,
    handler: Arc<Mutex<dyn WindowHandler + Send>>,
}

impl WindowPlugin {
    pub fn new(handler: Arc<Mutex<dyn WindowHandler + Send>>) -> Self {
        Self {
            channel: Weak::new(),
            handler,
        }
    }
}

impl Plugin for WindowPlugin {
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
    handler: Arc<Mutex<dyn WindowHandler + Send>>,
}

impl MethodCallHandler for Handler {
    fn on_method_call(&mut self, call: MethodCall) {
        match call.method().as_str() {
            "maximize" => {
                self.handler.lock().maximize();
                call.success_empty()
            }
            "iconify" => {
                self.handler.lock().iconify();
                call.success_empty()
            }
            "restore" => {
                self.handler.lock().restore();
                call.success_empty()
            }
            "isMaximized" => call.success(self.handler.lock().is_maximized()),
            "isIconified" => call.success(self.handler.lock().is_iconified()),
            "isVisible" => call.success(self.handler.lock().is_visible()),
            "show" => {
                self.handler.lock().show();
                call.success_empty()
            }
            "hide" => {
                self.handler.lock().hide();
                call.success_empty()
            }
            "close" => {
                self.handler.lock().close();
                call.success_empty()
            }
            "set_pos" => {
                let args: PositionParams = call.args();
                self.handler.lock().set_pos(args);
                call.success_empty()
            }
            "get_pos" => call.success(self.handler.lock().get_pos()),
            "start_drag" => {
                self.handler.lock().start_drag();
                call.success_empty()
            }
            "end_drag" => {
                self.handler.lock().end_drag();
                call.success_empty()
            }
            _ => call.not_implemented(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct PositionParams {
    pub x: f32,
    pub y: f32,
}
