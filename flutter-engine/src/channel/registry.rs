use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, Weak},
};

use log::{trace, warn};

use crate::{desktop_window_state::InitData, ffi::PlatformMessage};

use super::Channel;

pub struct ChannelRegistry {
    channels: HashMap<String, Arc<dyn Channel>>,
    init_data: Weak<InitData>,
}

pub struct ChannelRegistrar<'a> {
    plugin_name: &'static str,
    init_data: &'a Weak<InitData>,
    channels: &'a mut HashMap<String, Arc<dyn Channel>>,
}

impl ChannelRegistry {
    pub fn new(init_data: Weak<InitData>) -> Self {
        Self {
            channels: HashMap::new(),
            init_data,
        }
    }

    pub fn with_channel_registrar<F>(&mut self, plugin_name: &'static str, f: F)
    where
        F: FnOnce(&mut ChannelRegistrar),
    {
        let mut registrar = ChannelRegistrar {
            plugin_name,
            init_data: &self.init_data,
            channels: &mut self.channels,
        };
        f(&mut registrar);
    }

    pub fn with_channel<F>(&self, channel_name: &'static str, mut f: F)
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
            channel.handle_method(message);
        } else {
            warn!(
                "No plugin registered to handle messages from channel: {}",
                &message.channel
            );
            if let Some(handle) = message.response_handle.take() {
                self.init_data
                    .upgrade()
                    .unwrap()
                    .engine
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
        channel.init(Weak::clone(&self.init_data), self.plugin_name);
        let name = channel.name().to_owned();
        let arc = Arc::new(channel);
        let weak = Arc::downgrade(&arc);
        self.channels.insert(name, arc);
        weak
    }
}
