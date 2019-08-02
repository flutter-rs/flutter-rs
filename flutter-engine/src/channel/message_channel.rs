use std::sync::{Arc, RwLock, Weak};

use crate::{
    channel::{Channel, MethodCallHandler},
    codec::MethodCodec,
    desktop_window_state::InitData,
};

use log::error;

pub struct MessageChannel {
    name: &'static str,
    init_data: Weak<InitData>,
    plugin_name: Option<&'static str>,
    codec: &'static dyn MethodCodec,
}

impl MessageChannel {
    pub fn new(name: &'static str, codec: &'static dyn MethodCodec) -> Self {
        Self {
            name,
            init_data: Weak::new(),
            plugin_name: None,
            codec,
        }
    }
}

impl Channel for MessageChannel {
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
        None
    }

    fn plugin_name(&self) -> &'static str {
        self.plugin_name.unwrap()
    }

    fn codec(&self) -> &MethodCodec {
        self.codec
    }
}
