use std::sync::{Arc, RwLock, Weak};

use crate::{
    channel::{Channel, MethodCallHandler},
    codec::{standard_codec::CODEC, MethodCodec},
    desktop_window_state::RuntimeData,
    ffi::FlutterEngine,
};

use log::error;

pub struct StandardMethodChannel {
    name: String,
    engine: Weak<FlutterEngine>,
    method_handler: Weak<RwLock<MethodCallHandler + Send + Sync>>,
    plugin_name: Option<&'static str>,
}

impl StandardMethodChannel {
    pub fn new(name: &str, method_handler: Weak<RwLock<MethodCallHandler + Send + Sync>>) -> Self {
        Self {
            name: name.to_owned(),
            engine: Weak::new(),
            method_handler,
            plugin_name: None,
        }
    }

    pub fn set_handler(&mut self, method_handler: Weak<RwLock<MethodCallHandler + Send + Sync>>) {
        self.method_handler = method_handler;
    }
}

impl Channel for StandardMethodChannel {
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
        self.method_handler.upgrade()
    }

    fn plugin_name(&self) -> &'static str {
        self.plugin_name.unwrap()
    }

    fn codec(&self) -> &MethodCodec {
        &CODEC
    }
}
