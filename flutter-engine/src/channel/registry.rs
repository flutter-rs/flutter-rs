use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, Weak},
};

use log::{trace, warn};

use crate::{FlutterEngineWeakRef, PlatformMessage};

use super::Channel;

#[derive(Default)]
pub struct ChannelRegistry {
    channels: HashMap<String, Arc<dyn Channel>>,
    engine: FlutterEngineWeakRef,
}

pub struct ChannelRegistrar<'a> {
    plugin_name: &'static str,
    engine: &'a FlutterEngineWeakRef,
    channels: &'a mut HashMap<String, Arc<dyn Channel>>,
}

impl ChannelRegistry {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn init(&mut self, engine: FlutterEngineWeakRef) {
        self.engine = engine;
    }

    pub fn remove_channel(&mut self, channel_name: &str) -> Option<Arc<dyn Channel>> {
        self.channels.remove(channel_name)
    }

    pub fn with_channel_registrar<F>(&mut self, plugin_name: &'static str, f: F)
    where
        F: FnOnce(&mut ChannelRegistrar),
    {
        let mut registrar = ChannelRegistrar {
            plugin_name,
            engine: &self.engine,
            channels: &mut self.channels,
        };
        f(&mut registrar);
    }

    pub fn with_channel<F>(&self, channel_name: &str, mut f: F)
    where
        F: FnMut(&dyn Channel),
    {
        if let Some(channel) = self.channels.get(channel_name) {
            f(&**channel);
        }
    }

    pub fn handle(&mut self, mut message: PlatformMessage) {
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

impl<'a> ChannelRegistrar<'a> {
    pub fn register_channel<C>(&mut self, mut channel: C) -> Weak<C>
    where
        C: Channel + 'static,
    {
        channel.init(self.engine.clone(), self.plugin_name);
        let name = channel.name().to_owned();
        let arc = Arc::new(channel);
        let weak = Arc::downgrade(&arc);
        self.channels.insert(name, arc);
        weak
    }
}
