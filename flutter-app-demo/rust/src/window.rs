//! Plugin to handle system dialogs.
//! It handles flutter-rs/dialog type message.

use std::{
    cell::RefCell,
    sync::{Arc, Mutex, Weak},
};

use flutter_engine::{
    channel::{Channel, JsonMethodChannel},
    codec::{json_codec::Value, MethodCallResult},
    plugins::{Plugin, PluginChannel},
    PlatformMessage, RuntimeData, Window,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

const CHANNEL_NAME: &str = "flutter-rs/window";

pub struct WindowPlugin {
    channel: Arc<Mutex<JsonMethodChannel>>,
    state: RefCell<WindowState>,
}

impl PluginChannel for WindowPlugin {
    fn channel_name() -> &'static str {
        CHANNEL_NAME
    }
}

impl WindowPlugin {
    pub fn new() -> Self {
        WindowPlugin {
            channel: Arc::new(Mutex::new(JsonMethodChannel::new(CHANNEL_NAME))),
            state: RefCell::new(WindowState::new()),
        }
    }

    pub fn drag_window(&self, window: &mut Window, x: f64, y: f64) -> bool {
        let state = self.state.borrow();
        if state.dragging {
            let (wx, wy) = window.get_pos();
            let dx = (x - state.start_cursor_pos.0) as i32;
            let dy = (y - state.start_cursor_pos.1) as i32;
            window.set_pos(wx + dx, wy + dy);
        }
        state.dragging
    }
}

impl Plugin for WindowPlugin {
    fn init_channel(&mut self, registry: Weak<RuntimeData>) {
        let mut channel = self.channel.lock().unwrap();
        channel.init(registry);
    }

    fn handle(&mut self, msg: &mut PlatformMessage, window: &mut Window) {
        let channel = self.channel.lock().unwrap();
        let decoded = channel.decode_method_call(msg).unwrap();
        let handle = &mut msg.response_handle;

        let s = serde_json::to_string(&decoded.args);

        match decoded.method.as_str() {
            "maximize" => {
                window.maximize();
            }
            "iconify" => {
                window.iconify();
            }
            "restore" => {
                window.restore();
            }
            "show" => {
                window.show();
            }
            "hide" => {
                window.hide();
            }
            "close" => {
                window.set_should_close(true);
            }
            "set_pos" => {
                let params: serde_json::Result<PositionParams> = serde_json::from_str(&s.unwrap());
                if params.is_err() {
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
                let PositionParams { x, y } = params;
                window.set_pos(x as i32, y as i32);
            }
            "get_pos" => {
                let (xpos, ypos) = window.get_pos();
                channel.send_method_call_response(
                    handle,
                    MethodCallResult::Ok(json!({"x": xpos, "y": ypos})),
                );
                return;
            }
            "start_drag" => {
                let mut state = self.state.borrow_mut();
                let pos = window.get_cursor_pos();
                state.dragging = true;
                state.start_cursor_pos = pos;
            }
            "end_drag" => {
                let mut state = self.state.borrow_mut();
                state.dragging = false;
            }
            _ => {}
        }

        channel.send_method_call_response(handle, MethodCallResult::Ok(Value::Null));
    }
}

#[derive(Serialize, Deserialize)]
struct PositionParams {
    x: f32,
    y: f32,
}

struct WindowState {
    dragging: bool,
    start_cursor_pos: (f64, f64),
}

impl WindowState {
    fn new() -> Self {
        WindowState {
            dragging: false,
            start_cursor_pos: (0.0, 0.0),
        }
    }
}
