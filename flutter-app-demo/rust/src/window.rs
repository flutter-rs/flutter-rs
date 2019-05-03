//! Plugin to handle system dialogs.
//! It handles flutter-rs/dialog type message.
use flutter_engine::plugins::prelude::*;

const PLUGIN_NAME: &str = module_path!();
const CHANNEL_NAME: &str = "flutter-rs/window";

pub struct WindowPlugin {
    channel: Weak<JsonMethodChannel>,
    state: Arc<RwLock<WindowState>>,
}

impl Plugin for WindowPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, registrar: &mut ChannelRegistrar) {
        let method_handler = Arc::downgrade(&self.state);
        self.channel =
            registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, method_handler));
    }
}

impl WindowPlugin {
    pub fn new() -> Self {
        WindowPlugin {
            channel: Weak::new(),
            state: Arc::new(RwLock::new(WindowState::new())),
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

impl MethodCallHandler for WindowState {
    fn on_method_call(
        &mut self,
        call: MethodCall,
        runtime_data: RuntimeData,
    ) -> Result<Value, MethodCallError> {
        match call.method.as_str() {
            "maximize" => {
                runtime_data.with_window(|window| {
                    window.maximize();
                })?;
                Ok(Value::Null)
            }
            "iconify" => {
                runtime_data.with_window(|window| {
                    window.iconify();
                })?;
                Ok(Value::Null)
            }
            "restore" => {
                runtime_data.with_window(|window| {
                    window.restore();
                })?;
                Ok(Value::Null)
            }
            "show" => {
                runtime_data.with_window(|window| {
                    window.show();
                })?;
                Ok(Value::Null)
            }
            "hide" => {
                runtime_data.with_window(|window| {
                    window.hide();
                })?;
                Ok(Value::Null)
            }
            "close" => {
                runtime_data.with_window(|window| {
                    window.set_should_close(true);
                })?;
                Ok(Value::Null)
            }
            "set_pos" => {
                let args: PositionParams = from_value(&call.args)?;
                runtime_data.with_window(move |window| {
                    window.set_pos(args.x as i32, args.y as i32);
                })?;
                Ok(Value::Null)
            }
            "get_pos" => {
                let (xpos, ypos) = runtime_data.with_window_result(|window| window.get_pos())?;
                Ok(json_value!({"x": xpos, "y": ypos}))
            }
            "start_drag" => {
                let pos = runtime_data.with_window_result(|window| window.get_cursor_pos())?;
                self.dragging = true;
                self.start_cursor_pos = pos;
                Ok(Value::Null)
            }
            "end_drag" => {
                self.dragging = false;
                Ok(Value::Null)
            }
            _ => Err(MethodCallError::NotImplemented),
        }
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
