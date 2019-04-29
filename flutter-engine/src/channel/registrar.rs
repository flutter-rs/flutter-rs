use super::Channel;
use crate::{desktop_window_state::RuntimeData, ffi::PlatformMessage};

use std::{
    any::Any,
    collections::HashMap,
    ops::Deref,
    sync::{Arc, Weak},
};

use log::{trace, warn};

pub struct ChannelRegistrar {
    channels: HashMap<String, Arc<dyn Channel>>,
    runtime_data: Weak<RuntimeData>,
}

impl ChannelRegistrar {
    pub fn new(runtime_data: Weak<RuntimeData>) -> Self {
        Self {
            channels: HashMap::new(),
            runtime_data,
        }
    }

    pub fn register_channel<C>(&mut self, mut channel: C) -> Weak<C>
    where
        C: Channel + 'static,
    {
        channel.init(Weak::clone(&self.runtime_data));
        let name = channel.name().to_owned();
        let arc = Arc::new(channel);
        let weak = Arc::downgrade(&arc);
        self.channels.insert(name, arc);
        weak
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
