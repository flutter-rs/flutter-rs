use std::sync::{Arc, Weak};

use crate::{
    channel::Channel,
    codec::standard_codec::{StandardMethodCodec, Value},
    desktop_window_state::RuntimeData,
    ffi::FlutterEngine,
};

use log::error;

pub struct StandardMethodChannel {
    name: String,
    engine: Weak<FlutterEngine>,
}

impl StandardMethodChannel {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            engine: Weak::new(),
        }
    }
}

impl Channel for StandardMethodChannel {
    type R = Value;
    type Codec = StandardMethodCodec;

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
}
