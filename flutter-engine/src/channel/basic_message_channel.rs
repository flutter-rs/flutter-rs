use std::sync::{Arc, RwLock, Weak};

use crate::{channel::{ChannelImpl, MessageChannel, MessageHandler}, codec::MessageCodec, FlutterEngineWeakRef, FlutterEngine};

use log::error;

pub struct BasicMessageChannel {
    name: String,
    engine: FlutterEngineWeakRef,
    message_handler: Weak<RwLock<dyn MessageHandler + Send + Sync>>,
    plugin_name: Option<&'static str>,
    codec: &'static dyn MessageCodec,
}

impl BasicMessageChannel {
    pub fn new<N: AsRef<str>>(
        name: N,
        message_handler: Weak<RwLock<dyn MessageHandler + Send + Sync>>,
        codec: &'static dyn MessageCodec,
    ) -> Self {
        Self {
            name: name.as_ref().to_owned(),
            engine: Default::default(),
            message_handler,
            plugin_name: None,
            codec,
        }
    }

    pub fn set_handler(&mut self, message_handler: Weak<RwLock<dyn MessageHandler + Send + Sync>>) {
        self.message_handler = message_handler;
    }
}

impl ChannelImpl for BasicMessageChannel {
    fn name(&self) -> &str {
        self.name.as_ref()
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

impl MessageChannel for BasicMessageChannel {
    fn message_handler(&self) -> Option<Arc<RwLock<dyn MessageHandler + Send + Sync>>> {
        self.message_handler.upgrade()
    }

    fn codec(&self) -> &'static dyn MessageCodec {
        self.codec
    }
}

message_channel!(BasicMessageChannel);
