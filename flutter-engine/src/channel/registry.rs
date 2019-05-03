use super::Channel;
use crate::{desktop_window_state::RuntimeData, ffi::PlatformMessage};

use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, Weak},
};

use log::{trace, warn};

pub struct ChannelRegistry {
    channels: HashMap<String, Arc<dyn Channel>>,
    runtime_data: Weak<RuntimeData>,
}

pub struct ChannelRegistrar<'a> {
    plugin_name: &'static str,
    runtime_data: &'a Weak<RuntimeData>,
    channels: &'a mut HashMap<String, Arc<dyn Channel>>,
}

impl ChannelRegistry {
    pub fn new(runtime_data: Weak<RuntimeData>) -> Self {
        Self {
            channels: HashMap::new(),
            runtime_data,
        }
    }

    pub fn with_channel_registrar<F>(&mut self, plugin_name: &'static str, f: F)
    where
        F: FnOnce(&mut ChannelRegistrar),
    {
        let mut registrar = ChannelRegistrar {
            plugin_name,
            runtime_data: &self.runtime_data,
            channels: &mut self.channels,
        };
        f(&mut registrar);
    }

    pub fn handle(&mut self, mut message: PlatformMessage) {
        let runtime_data = self.runtime_data.upgrade().unwrap();
        let window = runtime_data.window();
        if let Some(channel) = self.channels.get(message.channel.deref()) {
            trace!("Processing message from channel: {}", message.channel);
            channel.handle_method(&mut message, window);
        } else {
            warn!(
                "No plugin registered to handle messages from channel: {}",
                &message.channel
            );
        }
        if let Some(handle) = message.response_handle.take() {
            warn!(
                "No response for channel {}, sending default empty response",
                message.channel
            );
            runtime_data
                .engine
                .send_platform_message_response(handle, &[]);
        }
    }
}

impl<'a> ChannelRegistrar<'a> {
    pub fn register_channel<C>(&mut self, mut channel: C) -> Weak<C>
    where
        C: Channel + 'static,
    {
        channel.init(Weak::clone(&self.runtime_data), self.plugin_name);
        let name = channel.name().to_owned();
        let arc = Arc::new(channel);
        let weak = Arc::downgrade(&arc);
        self.channels.insert(name, arc);
        weak
    }
}
