//! Register plugin with this registry to listen to flutter MethodChannel calls.

use std::{
    any::Any,
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::{Arc, RwLock},
};

use crate::{
    channel::{ChannelRegistrar, ChannelRegistry},
    FlutterEngineWeakRef, PlatformMessage,
};

#[derive(Default)]
pub struct PluginRegistrar {
    plugins: HashMap<String, Arc<RwLock<dyn Any>>>,
    pub channel_registry: ChannelRegistry,
}

impl PluginRegistrar {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn init(&mut self, engine: FlutterEngineWeakRef) {
        self.channel_registry.init(engine);
    }

    pub fn add_plugin<P>(&mut self, plugin: P) -> &mut Self
    where
        P: Plugin + 'static,
    {
        let arc = Arc::new(RwLock::new(plugin));
        {
            let mut plugin = arc.write().unwrap();
            self.channel_registry
                .with_channel_registrar(P::plugin_name(), |registrar| {
                    plugin.init_channels(registrar);
                });
        }
        self.plugins.insert(P::plugin_name().to_owned(), arc);
        self
    }

    pub fn handle(&mut self, message: PlatformMessage) {
        self.channel_registry.handle(message);
    }

    pub fn with_plugin<F, P>(&self, f: F)
    where
        F: FnOnce(&P),
        P: Plugin + 'static,
    {
        if let Some(arc) = self.plugins.get(P::plugin_name()) {
            let plugin = arc.read().unwrap();
            let plugin = plugin.deref().downcast_ref::<P>().unwrap();
            f(plugin);
        }
    }

    pub fn with_plugin_mut<F, P>(&mut self, f: F)
    where
        F: FnOnce(&mut P),
        P: Plugin + 'static,
    {
        if let Some(arc) = self.plugins.get_mut(P::plugin_name()) {
            let mut plugin = arc.write().unwrap();
            let plugin = plugin.deref_mut().downcast_mut::<P>().unwrap();
            f(plugin);
        }
    }
}

pub trait Plugin {
    fn plugin_name() -> &'static str;
    fn init_channels(&mut self, registrar: &mut ChannelRegistrar);
}
