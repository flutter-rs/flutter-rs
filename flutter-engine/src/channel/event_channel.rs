use std::sync::{Arc, RwLock, Weak};

use crate::{
    channel::{Channel, EventHandler, MethodCallHandler},
    codec::{standard_codec::CODEC, MethodCall, MethodCodec, Value},
    desktop_window_state::RuntimeData,
    error::MethodCallError,
    ffi::FlutterEngine,
    Window,
};

use log::error;

pub struct EventChannel {
    name: String,
    engine: Weak<FlutterEngine>,
    method_handler: Arc<RwLock<MethodCallHandler + Send + Sync>>,
    plugin_name: Option<&'static str>,
}

struct EventChannelMethodCallHandler {
    event_handler: Weak<RwLock<EventHandler + Send + Sync>>,
}

impl EventChannel {
    pub fn new(name: &str, handler: Weak<RwLock<EventHandler + Send + Sync>>) -> Self {
        Self {
            name: name.to_owned(),
            engine: Weak::new(),
            method_handler: Arc::new(RwLock::new(EventChannelMethodCallHandler::new(handler))),
            plugin_name: None,
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

    fn init(&mut self, runtime_data: Weak<RuntimeData>, plugin_name: &'static str) {
        if self.engine.upgrade().is_some() {
            error!("Channel {} was already initialized", self.name);
        }
        if let Some(runtime_data) = runtime_data.upgrade() {
            self.engine = Arc::downgrade(&runtime_data.engine);
        }
        self.plugin_name.replace(plugin_name);
    }

    fn method_handler(&self) -> Option<Arc<RwLock<MethodCallHandler + Send + Sync>>> {
        Some(Arc::clone(&self.method_handler))
    }

    fn plugin_name(&self) -> &'static str {
        self.plugin_name.unwrap()
    }

    fn codec(&self) -> &MethodCodec {
        &CODEC
    }
}

impl EventChannelMethodCallHandler {
    pub fn new(handler: Weak<RwLock<EventHandler + Send + Sync>>) -> Self {
        Self {
            event_handler: handler,
        }
    }
}

impl MethodCallHandler for EventChannelMethodCallHandler {
    fn on_method_call(
        &mut self,
        channel: &str,
        call: MethodCall,
        _: &mut Window,
    ) -> Result<Value, MethodCallError> {
        if let Some(handler) = self.event_handler.upgrade() {
            let mut handler = handler.write().unwrap();
            match call.method.as_str() {
                "listen" => handler.on_listen(channel, call.args),
                "cancel" => handler.on_cancel(channel),
                _ => Err(MethodCallError::NotImplemented),
            }
        } else {
            Err(MethodCallError::ChannelClosed)
        }
    }
}
