use std::sync::{Arc, RwLock, Weak};

use crate::{
    channel::{Channel, MethodCallHandler},
    codec::{json_codec::CODEC, MethodCodec},
    desktop_window_state::InitData,
};

use log::error;

pub struct JsonMethodChannel {
    name: &'static str,
    init_data: Weak<InitData>,
    method_handler: Weak<RwLock<MethodCallHandler + Send + Sync>>,
    plugin_name: Option<&'static str>,
}

impl JsonMethodChannel {
    pub fn new(
        name: &'static str,
        method_handler: Weak<RwLock<MethodCallHandler + Send + Sync>>,
    ) -> Self {
        Self {
            name,
            init_data: Weak::new(),
            method_handler,
            plugin_name: None,
        }
    }

    pub fn set_handler(&mut self, method_handler: Weak<RwLock<MethodCallHandler + Send + Sync>>) {
        self.method_handler = method_handler;
    }
}

impl Channel for JsonMethodChannel {
    fn name(&self) -> &'static str {
        &self.name
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
