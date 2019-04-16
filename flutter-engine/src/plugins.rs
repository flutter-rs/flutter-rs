//! Register plugin with this registry to listen to flutter MethodChannel calls.

mod platform;
mod textinput;
//pub mod dialog;
//pub mod window;
//pub mod navigation;

use crate::{desktop_window_state::RuntimeData, ffi::PlatformMessage};

use std::{
    borrow::{Borrow, BorrowMut},
    cell::RefCell,
    collections::HashMap,
    ops::DerefMut,
    rc::Weak,
};

use log::{trace, warn};

pub struct PluginRegistrar {
    plugins: HashMap<String, Box<dyn Plugin>>,
    runtime_data: Weak<RuntimeData>,
}

impl PluginRegistrar {
    pub fn new(runtime_data: Weak<RuntimeData>) -> Self {
        Self {
            plugins: HashMap::new(),
            runtime_data,
        }
    }

    pub fn add_system_plugins(&mut self) {
        self.add_plugin(platform::PlatformPlugin::new())
            .add_plugin(textinput::TextInputPlugin::new());
    }

    pub fn add_plugin<P>(&mut self, mut plugin: P) -> &mut Self
    where
        P: Plugin + PluginChannel + 'static,
    {
        plugin.init_channel(Weak::clone(&self.runtime_data));
        self.plugins
            .insert(P::channel_name().to_owned(), Box::new(plugin));
        self
    }

    pub fn handle(&mut self, message: PlatformMessage) {
        let mut message_handled = false;
        let runtime_data = self.runtime_data.upgrade().unwrap();
        let mut window = RefCell::borrow_mut(&runtime_data.window);
        for (channel, plugin) in &mut self.plugins {
            if channel == &message.channel {
                trace!("Processing message from channel: {}", channel);
                plugin.handle(&message, window.deref_mut());
                message_handled = true;
                break;
            }
        }
        if !message_handled {
            warn!(
                "No plugin registered to handle messages from channel: {}",
                &message.channel
            );
        }
    }

    pub fn get_plugin<P>(&self) -> Option<&P>
    where
        P: Plugin + PluginChannel + 'static,
        Box<dyn Plugin>: Borrow<P>,
    {
        self.plugins.get(P::channel_name()).map(Box::borrow)
    }

    pub fn get_plugin_mut<P>(&mut self) -> Option<&mut P>
    where
        P: Plugin + PluginChannel + 'static,
        Box<dyn Plugin>: BorrowMut<P>,
    {
        self.plugins.get_mut(P::channel_name()).map(Box::borrow_mut)
    }
}

pub trait PluginChannel {
    fn channel_name() -> &'static str;
}

pub trait Plugin {
    fn init_channel(&mut self, registar: Weak<RuntimeData>);
    fn handle(&mut self, message: &PlatformMessage, window: &mut glfw::Window);
}
