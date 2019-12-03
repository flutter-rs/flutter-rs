use std::sync::{Arc, RwLock, Weak};

use flutter_engine_codec::MessageCodec;

use crate::{
    channel::{ChannelImpl, MessageChannel, MessageHandler},
    desktop_window_state::InitData,
};

use log::error;

pub struct BasicMessageChannel {
    name: String,
    init_data: Weak<InitData>,
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
            init_data: Weak::new(),
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

impl MessageChannel for BasicMessageChannel {
    fn message_handler(&self) -> Option<Arc<RwLock<dyn MessageHandler + Send + Sync>>> {
        self.message_handler.upgrade()
    }

    fn codec(&self) -> &'static dyn MessageCodec {
        self.codec
    }
}

message_channel!(BasicMessageChannel);
