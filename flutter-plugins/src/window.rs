//! Plugin for the `flutter-rs/window` channel.
//!
//! Lets Dart drive the host window: show and hide it, maximize, iconify and
//! restore it, read and set its position, and drag it by its own chrome.
use std::sync::{Arc, Weak};

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
const CHANNEL_NAME: &str = "flutter-rs/window";

/// The embedder half of the `flutter-rs/window` channel.
pub trait WindowHandler {
    /// Close the window.
    fn close(&mut self);

    /// Make the window visible.
    fn show(&mut self);

    /// Make the window invisible, without closing it.
    fn hide(&mut self);

    /// Maximize the window.
    fn maximize(&mut self);

    /// Minimize the window.
    fn iconify(&mut self);

    /// Undo a maximize or an iconify.
    fn restore(&mut self);

    /// Whether the window is maximized.
    fn is_maximized(&mut self) -> bool;

    /// Whether the window is minimized.
    fn is_iconified(&mut self) -> bool;

    /// Whether the window is visible.
    fn is_visible(&mut self) -> bool;

    /// Move the window to `pos`, in screen coordinates.
    fn set_pos(&mut self, pos: PositionParams);

    /// The window's position, in screen coordinates.
    fn get_pos(&mut self) -> PositionParams;

    /// Begin dragging the window with the pointer.
    fn start_drag(&mut self);

    /// Stop dragging the window.
    fn end_drag(&mut self);
}

/// Plugin for the `flutter-rs/window` channel.
pub struct WindowPlugin {
    channel: Weak<MethodChannel>,
    handler: Arc<Mutex<dyn WindowHandler + Send>>,
}

impl WindowPlugin {
    /// A plugin that serves the channel out of `handler`.
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
        match call.method() {
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
                let Ok(args) = call.args::<PositionParams>() else {
                    return call.error(
                        "invalid-args",
                        "could not deserialize arguments",
                        Value::Null,
                    );
                };
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

/// A window position, in screen coordinates.
#[derive(Serialize, Deserialize)]
pub struct PositionParams {
    /// Distance from the left edge of the screen.
    pub x: f32,
    /// Distance from the top edge of the screen.
    pub y: f32,
}
