//! Register plugin with this registry to listen to flutter MethodChannel calls.

mod platform;
mod textinput;
//pub mod dialog;
//pub mod window;
mod navigation;

pub use self::{
    navigation::NavigationPlugin, platform::PlatformPlugin, textinput::TextInputPlugin,
};

use crate::{desktop_window_state::RuntimeData, ffi::PlatformMessage};

use std::{
    borrow::{Borrow, BorrowMut},
    collections::HashMap,
    sync::Weak,
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
            .add_plugin(textinput::TextInputPlugin::new())
            .add_plugin(navigation::NavigationPlugin::new());
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
        let window = runtime_data.window();
        for (channel, plugin) in &mut self.plugins {
            if channel == &message.channel {
                trace!("Processing message from channel: {}", channel);
                plugin.handle(&message, window);
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

    pub fn with_plugin<F, P>(&self, mut f: F)
    where
        F: FnMut(&P),
        P: Plugin + PluginChannel + 'static,
    {
        if let Some(b) = self.plugins.get(P::channel_name()) {
            unsafe {
                let plugin: &Box<P> = std::mem::transmute(b);
                f(plugin);
            }
        }
    }

    pub fn with_plugin_mut<F, P>(&mut self, mut f: F)
    where
        F: FnMut(&mut P),
        P: Plugin + PluginChannel + 'static,
    {
        if let Some(b) = self.plugins.get_mut(P::channel_name()) {
            unsafe {
                let plugin: &mut Box<P> = std::mem::transmute(b);
                f(plugin);
            }
        }
    }
}

pub trait PluginChannel {
    fn channel_name() -> &'static str;
}

pub trait Plugin {
    fn init_channel(&mut self, registrar: Weak<RuntimeData>);
    fn handle(&mut self, message: &PlatformMessage, window: &mut glfw::Window);
}
