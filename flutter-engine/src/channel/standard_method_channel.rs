use std::sync::{Arc, RwLock, Weak};

use log::error;

use crate::{
    channel::{ChannelImpl, MethodCallHandler, MethodChannel},
    codec::{standard_codec::CODEC, MethodCodec},
    FlutterEngine, FlutterEngineWeakRef,
};

pub struct StandardMethodChannel {
    name: String,
    engine: FlutterEngineWeakRef,
    method_handler: Weak<RwLock<dyn MethodCallHandler + Send + Sync>>,
    plugin_name: Option<&'static str>,
}

impl StandardMethodChannel {
    pub fn new<N: AsRef<str>>(
        name: N,
        method_handler: Weak<RwLock<dyn MethodCallHandler + Send + Sync>>,
    ) -> Self {
        Self {
            name: name.as_ref().to_owned(),
            engine: Default::default(),
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

impl ChannelImpl for StandardMethodChannel {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn engine(&self) -> Option<FlutterEngine> {
        self.engine.upgrade()
    }

    fn init(&mut self, engine: FlutterEngineWeakRef, plugin_name: &'static str) {
        if self.engine.upgrade().is_some() {
            error!("Channel {} was already initialized", self.name);
        }
        self.engine = engine;
        self.plugin_name.replace(plugin_name);
    }

    fn plugin_name(&self) -> &'static str {
        self.plugin_name.unwrap()
    }
}

impl MethodChannel for StandardMethodChannel {
    fn method_handler(&self) -> Option<Arc<RwLock<dyn MethodCallHandler + Send + Sync>>> {
        self.method_handler.upgrade()
    }

    fn codec(&self) -> &'static dyn MethodCodec {
        &CODEC
    }
}

method_channel!(StandardMethodChannel);
