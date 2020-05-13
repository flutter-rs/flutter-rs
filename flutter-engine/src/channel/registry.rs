use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, Weak},
};

use log::{trace, warn};

use crate::FlutterEngineWeakRef;

use super::Channel;
use crate::channel::platform_message::PlatformMessage;

#[derive(Default)]
pub struct ChannelRegistry {
    channels: HashMap<String, Arc<dyn Channel>>,
    engine: FlutterEngineWeakRef,
}

impl ChannelRegistry {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn init(&mut self, engine: FlutterEngineWeakRef) {
        self.engine = engine;
    }

    pub fn register_channel<C>(&mut self, mut channel: C) -> Weak<C>
    where
        C: Channel + 'static,
    {
        channel.init(self.engine.clone());
        let name = channel.name().to_owned();
        let arc = Arc::new(channel);
        let weak = Arc::downgrade(&arc);
        self.channels.insert(name, arc);
        weak
    }

    pub fn remove_channel(&mut self, channel_name: &str) -> Option<Arc<dyn Channel>> {
        self.channels.remove(channel_name)
    }

    pub fn with_channel<F>(&self, channel_name: &str, f: F)
    where
        F: FnOnce(&dyn Channel),
    {
        if let Some(channel) = self.channels.get(channel_name) {
            f(&**channel);
        }
    }

    pub fn handle(&self, mut message: PlatformMessage) {
        if let Some(channel) = self.channels.get(message.channel.deref()) {
            trace!("Processing message from channel: {}", message.channel);
            channel.handle_platform_message(message);
        } else {
            warn!(
                "No plugin registered to handle messages from channel: {}",
                &message.channel
            );
            if let Some(handle) = message.response_handle.take() {
                self.engine
                    .upgrade()
                    .unwrap()
                    .send_platform_message_response(handle, &[]);
            }
        }
    }
}
