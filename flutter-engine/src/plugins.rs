//! Register plugin with this registry to listen to flutter MethodChannel calls.

use std::{
    any::Any,
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use parking_lot::RwLock;

use crate::FlutterEngine;

/// Keeps one instance of each plugin type, keyed by [`Plugin::plugin_name`].
#[derive(Default)]
pub struct PluginRegistrar {
    plugins: HashMap<String, Arc<RwLock<dyn Any>>>,
}

impl core::fmt::Debug for PluginRegistrar {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PluginRegistrar")
            .field("plugins", &self.plugins.keys())
            .finish_non_exhaustive()
    }
}

impl PluginRegistrar {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialise `plugin` against `engine` and keep it.
    ///
    /// Registering two plugins whose [`Plugin::plugin_name`] matches is a bug:
    /// the first one is dropped, which silently kills its channels, so it is
    /// logged.
    pub fn add_plugin<P>(&mut self, engine: &FlutterEngine, plugin: P) -> &mut Self
    where
        P: Plugin + 'static,
    {
        let arc = Arc::new(RwLock::new(plugin));
        {
            let mut plugin = arc.write();
            plugin.init(engine);
        }
        if self
            .plugins
            .insert(P::plugin_name().to_owned(), arc)
            .is_some()
        {
            log::warn!(
                "plugin {} was already registered; the previous registration is now dead",
                P::plugin_name()
            );
        }
        self
    }

    /// Run `f` against the registered plugin of type `P`, if there is one.
    pub fn with_plugin<F, P>(&self, f: F)
    where
        F: FnOnce(&P),
        P: Plugin + 'static,
    {
        if let Some(arc) = self.plugins.get(P::plugin_name()) {
            let plugin = arc.read();
            match plugin.deref().downcast_ref::<P>() {
                Some(plugin) => f(plugin),
                None => log::error!("plugin name collision on {}", P::plugin_name()),
            }
        }
    }

    /// Run `f` against the registered plugin of type `P`, if there is one.
    pub fn with_plugin_mut<F, P>(&mut self, f: F)
    where
        F: FnOnce(&mut P),
        P: Plugin + 'static,
    {
        if let Some(arc) = self.plugins.get_mut(P::plugin_name()) {
            let mut plugin = arc.write();
            match plugin.deref_mut().downcast_mut::<P>() {
                Some(plugin) => f(plugin),
                None => log::error!("plugin name collision on {}", P::plugin_name()),
            }
        }
    }
}

/// Something that registers channels on an engine and answers calls on them.
pub trait Plugin {
    /// The registry key for this plugin type; must be unique per process.
    fn plugin_name() -> &'static str;

    /// Register the plugin's channels. Called on the platform thread.
    fn init(&mut self, engine: &FlutterEngine);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter {
        calls: u32,
    }

    impl Plugin for Counter {
        fn plugin_name() -> &'static str {
            "test/counter"
        }

        fn init(&mut self, _engine: &FlutterEngine) {
            unreachable!("init needs a live engine and is not exercised here");
        }
    }

    /// `with_plugin*` must be a no-op for a plugin that was never added, rather
    /// than panicking on a missing key.
    #[test]
    fn an_unregistered_plugin_is_skipped() {
        let mut registrar = PluginRegistrar::new();
        let mut seen = false;
        registrar.with_plugin(|_: &Counter| seen = true);
        registrar.with_plugin_mut(|_: &mut Counter| seen = true);
        assert!(!seen);
    }

    /// The downcast must not panic when two plugin types share a name; the
    /// mismatch is reported and the call skipped.
    #[test]
    fn a_name_collision_does_not_panic() {
        struct Other;
        impl Plugin for Other {
            fn plugin_name() -> &'static str {
                "test/counter"
            }

            fn init(&mut self, _engine: &FlutterEngine) {}
        }

        let mut registrar = PluginRegistrar::new();
        registrar
            .plugins
            .insert("test/counter".to_owned(), Arc::new(RwLock::new(Other)));

        let mut seen = false;
        registrar.with_plugin(|_: &Counter| seen = true);
        registrar.with_plugin_mut(|_: &mut Counter| seen = true);
        assert!(!seen);
    }

    /// A plugin is reachable by its own type after being inserted, by `&` and
    /// by `&mut`.
    #[test]
    fn a_registered_plugin_is_reachable_by_type() {
        let mut registrar = PluginRegistrar::new();
        registrar.plugins.insert(
            Counter::plugin_name().to_owned(),
            Arc::new(RwLock::new(Counter { calls: 0 })),
        );

        registrar.with_plugin_mut(|plugin: &mut Counter| plugin.calls += 1);
        let mut calls = 0;
        registrar.with_plugin(|plugin: &Counter| calls = plugin.calls);
        assert_eq!(calls, 1);
    }
}
