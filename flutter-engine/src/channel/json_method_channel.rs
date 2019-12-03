use std::sync::{Arc, RwLock, Weak};

use log::error;

use flutter_engine_codec::{json_codec::CODEC, MethodCodec};

use crate::{
    channel::{ChannelImpl, MethodCallHandler, MethodChannel},
    desktop_window_state::InitData,
};

pub struct JsonMethodChannel {
    name: String,
    init_data: Weak<InitData>,
    method_handler: Weak<RwLock<dyn MethodCallHandler + Send + Sync>>,
    plugin_name: Option<&'static str>,
}

impl JsonMethodChannel {
    pub fn new<N: AsRef<str>>(
        name: N,
        method_handler: Weak<RwLock<dyn MethodCallHandler + Send + Sync>>,
    ) -> Self {
        Self {
            name: name.as_ref().to_owned(),
            init_data: Weak::new(),
            method_handler,
            plugin_name: None,
        }
    }

    pub fn set_handler(
        &mut self,
        method_handler: Weak<RwLock<dyn MethodCallHandler + Send + Sync>>,
    ) {
        self.method_handler = method_handler;
    }
}

impl ChannelImpl for JsonMethodChannel {
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

impl MethodChannel for JsonMethodChannel {
    fn method_handler(&self) -> Option<Arc<RwLock<dyn MethodCallHandler + Send + Sync>>> {
        self.method_handler.upgrade()
    }

    fn codec(&self) -> &'static dyn MethodCodec {
        &CODEC
    }
}

method_channel!(JsonMethodChannel);
