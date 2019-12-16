//! Plugin to handle system dialogs.
//! It handles flutter-rs/dialog type message.
use super::prelude::*;
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
    channel: Weak<JsonMethodChannel>,
    handler: Arc<RwLock<Handler>>,
}

impl WindowPlugin {
    pub fn new(handler: Arc<Mutex<dyn WindowHandler + Send>>) -> Self {
        Self {
            channel: Weak::new(),
            handler: Arc::new(RwLock::new(Handler { handler })),
        }
    }
}

impl Plugin for WindowPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, registrar: &mut ChannelRegistrar) {
        let method_handler = Arc::downgrade(&self.handler);
        self.channel =
            registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, method_handler));
    }
}

struct Handler {
    handler: Arc<Mutex<dyn WindowHandler + Send>>,
}

impl MethodCallHandler for Handler {
    fn on_method_call(
        &mut self,
        call: MethodCall,
        _: FlutterEngine,
    ) -> Result<Value, MethodCallError> {
        match call.method.as_str() {
            "maximize" => {
                self.handler.lock().maximize();
                Ok(Value::Null)
            }
            "iconify" => {
                self.handler.lock().iconify();
                Ok(Value::Null)
            }
            "restore" => {
                self.handler.lock().restore();
                Ok(Value::Null)
            }
            "isMaximized" => Ok(Value::Boolean(self.handler.lock().is_maximized())),
            "isIconified" => Ok(Value::Boolean(self.handler.lock().is_iconified())),
            "isVisible" => Ok(Value::Boolean(self.handler.lock().is_visible())),
            "show" => {
                self.handler.lock().show();
                Ok(Value::Null)
            }
            "hide" => {
                self.handler.lock().hide();
                Ok(Value::Null)
            }
            "close" => {
                self.handler.lock().close();
                Ok(Value::Null)
            }
            "set_pos" => {
                let args: PositionParams = from_value(&call.args)?;
                self.handler.lock().set_pos(args);
                Ok(Value::Null)
            }
            "get_pos" => Ok(json_value!(self.handler.lock().get_pos())),
            "start_drag" => {
                self.handler.lock().start_drag();
                Ok(Value::Null)
            }
            "end_drag" => {
                self.handler.lock().end_drag();
                Ok(Value::Null)
            }
            _ => Err(MethodCallError::NotImplemented),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct PositionParams {
    pub x: f32,
    pub y: f32,
}
