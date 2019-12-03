use std::sync::{Arc, RwLock, Weak};

use log::error;

use flutter_engine_codec::{standard_codec::CODEC, MethodCall, MethodCodec, Value};

use crate::{
    channel::{ChannelImpl, EventHandler, MethodCallHandler, MethodChannel},
    desktop_window_state::{InitData, RuntimeData},
    error::MethodCallError,
};

pub struct EventChannel {
    name: String,
    init_data: Weak<InitData>,
    method_handler: Arc<RwLock<dyn MethodCallHandler + Send + Sync>>,
    plugin_name: Option<&'static str>,
}

struct EventChannelMethodCallHandler {
    event_handler: Weak<RwLock<dyn EventHandler + Send + Sync>>,
}

impl EventChannel {
    pub fn new<N: AsRef<str>>(
        name: N,
        handler: Weak<RwLock<dyn EventHandler + Send + Sync>>,
    ) -> Self {
        Self {
            name: name.as_ref().to_owned(),
            init_data: Weak::new(),
            method_handler: Arc::new(RwLock::new(EventChannelMethodCallHandler::new(handler))),
            plugin_name: None,
        }
    }
}

impl ChannelImpl for EventChannel {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn init_data(&self) -> Option<Arc<InitData>> {
        self.init_data.upgrade()
    }

    fn init(&mut self, init_data: Weak<InitData>, plugin_name: &'static str) {
        if self.init_data.upgrade().is_some() {
            error!("Channel {} was already initialized", self.name);
        }
        self.init_data = init_data;
        self.plugin_name.replace(plugin_name);
    }

    fn plugin_name(&self) -> &'static str {
        self.plugin_name.unwrap()
    }
}

impl MethodChannel for EventChannel {
    fn method_handler(&self) -> Option<Arc<RwLock<dyn MethodCallHandler + Send + Sync>>> {
        Some(Arc::clone(&self.method_handler))
    }

    fn codec(&self) -> &'static dyn MethodCodec {
        &CODEC
    }
}

impl EventChannelMethodCallHandler {
    pub fn new(handler: Weak<RwLock<dyn EventHandler + Send + Sync>>) -> Self {
        Self {
            event_handler: handler,
        }
    }
}

impl MethodCallHandler for EventChannelMethodCallHandler {
    fn on_method_call(
        &mut self,
        call: MethodCall,
        runtime_data: RuntimeData,
    ) -> Result<Value, MethodCallError> {
        if let Some(handler) = self.event_handler.upgrade() {
            let mut handler = handler.write().unwrap();
            match call.method.as_str() {
                "listen" => handler.on_listen(call.args, runtime_data),
                "cancel" => handler.on_cancel(runtime_data),
                _ => Err(MethodCallError::NotImplemented),
            }
        } else {
            Err(MethodCallError::ChannelClosed)
        }
    }
}

method_channel!(EventChannel);
