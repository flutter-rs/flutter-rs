use std::sync::{Arc, RwLock, Weak};

use crate::{
    channel::{Channel, MethodCallHandler},
    codec::{standard_codec::CODEC, MethodCodec, Value},
    desktop_window_state::RuntimeData,
    ffi::FlutterEngine,
    PlatformMessage, Window,
};

use crate::codec::MethodCallResult;
use log::{error, warn};

pub struct EventChannel {
    name: String,
    engine: Weak<FlutterEngine>,
    event_handler: Weak<RwLock<EventHandler>>,
}

impl EventChannel {
    pub fn new(name: &str, handler: Weak<RwLock<EventHandler>>) -> Self {
        Self {
            name: name.to_owned(),
            engine: Weak::new(),
            event_handler: handler,
        }
    }
}

impl Channel for EventChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn engine(&self) -> Option<Arc<FlutterEngine>> {
        self.engine.upgrade()
    }

    fn init(&mut self, runtime_data: Weak<RuntimeData>) {
        if self.engine.upgrade().is_some() {
            error!("Channel {} was already initialized", self.name);
        }
        if let Some(runtime_data) = runtime_data.upgrade() {
            self.engine = Arc::downgrade(&runtime_data.engine);
        }
    }

    fn method_handler(&self) -> Option<Arc<RwLock<MethodCallHandler>>> {
        None
    }

    fn codec(&self) -> &MethodCodec {
        &CODEC
    }

    fn handle_method(&self, msg: &mut PlatformMessage, window: &mut Window) {
        if let Some(handler) = self.event_handler.upgrade() {
            let mut handler = handler.write().unwrap();
            let decoded = self.decode_method_call(msg).unwrap();
            match decoded.method.as_str() {
                "listen" => {
                    let response = handler.on_listen(decoded.args);
                    self.send_method_call_response(&mut msg.response_handle, response);
                }
                "cancel" => {
                    let response = handler.on_cancel();
                    self.send_method_call_response(&mut msg.response_handle, response);
                }
                method => {
                    warn!(
                        "Unknown method {} called! Maybe this is not an event channel?",
                        method
                    );
                    self.send_method_call_response(
                        &mut msg.response_handle,
                        MethodCallResult::NotImplemented,
                    );
                }
            }
        }
    }
}

pub trait EventHandler {
    fn on_listen(&mut self, args: Value) -> MethodCallResult;
    fn on_cancel(&mut self) -> MethodCallResult;
}
