//! Plugin to handle system dialogs.
//! It handles flutter-rs/dialog type message.

use std::sync::RwLock;

use flutter_engine::plugins::prelude::*;

const PLUGIN_NAME: &str = module_path!();
const CHANNEL_NAME: &str = "flutter-rs/window";

pub struct WindowPlugin {
    channel: Weak<JsonMethodChannel>,
    state: RwLock<WindowState>,
}

method_call_args!(
    struct PositionArgs {
        @pub x: f64 = match map_value("x") {
            Value::F64(f) => f,
        },
        @pub y: f64 = match map_value("y") {
            Value::F64(f) => f,
        },
    }
);

impl Plugin for WindowPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, plugin: Weak<RwLock<Self>>, registrar: &mut ChannelRegistrar) {
        self.channel = registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, plugin));
    }
}

impl WindowPlugin {
    pub fn new() -> Self {
        WindowPlugin {
            channel: Weak::new(),
            state: RwLock::new(WindowState::new()),
        }
    }

    pub fn drag_window(&self, window: &mut Window, x: f64, y: f64) -> bool {
        let state = self.state.read().unwrap();
        if state.dragging {
            let (wx, wy) = window.get_pos();
            let dx = (x - state.start_cursor_pos.0) as i32;
            let dy = (y - state.start_cursor_pos.1) as i32;
            window.set_pos(wx + dx, wy + dy);
        }
        state.dragging
    }
}

impl MethodCallHandler for WindowPlugin {
    fn on_method_call(
        &mut self,
        _channel: &str,
        call: MethodCall,
        window: &mut Window,
    ) -> Result<Value, MethodCallError> {
        match call.method.as_str() {
            "maximize" => {
                window.maximize();
                Ok(Value::Null)
            }
            "iconify" => {
                window.iconify();
                Ok(Value::Null)
            }
            "restore" => {
                window.restore();
                Ok(Value::Null)
            }
            "show" => {
                window.show();
                Ok(Value::Null)
            }
            "hide" => {
                window.hide();
                Ok(Value::Null)
            }
            "close" => {
                window.set_should_close(true);
                Ok(Value::Null)
            }
            "set_pos" => {
                let args = PositionArgs::try_from(call.args)?;
                window.set_pos(args.x as i32, args.y as i32);
                Ok(Value::Null)
            }
            "get_pos" => {
                let (xpos, ypos) = window.get_pos();
                Ok(json_value!({"x": xpos, "y": ypos}))
            }
            "start_drag" => {
                let mut state = self.state.write().unwrap();
                let pos = window.get_cursor_pos();
                state.dragging = true;
                state.start_cursor_pos = pos;
                Ok(Value::Null)
            }
            "end_drag" => {
                let mut state = self.state.write().unwrap();
                state.dragging = false;
                Ok(Value::Null)
            }
            _ => Err(MethodCallError::NotImplemented),
        }
    }
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
