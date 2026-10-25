//! Plugin for the `flutter/navigation` channel.
//!
//! Drives the framework's `Navigator` from the host: the initial route at
//! startup, and pushes and pops after that.

use log::debug;
use std::sync::Weak;

use flutter_engine::channel::MethodCall;
use flutter_engine::codec::Value;
use flutter_engine::{
    FlutterEngine,
    channel::{MethodCallHandler, MethodChannel},
    codec::JSON_CODEC,
    plugins::Plugin,
};

/// The name this plugin is registered under.
pub const PLUGIN_NAME: &str = module_path!();
/// The channel this plugin handles.
pub const CHANNEL_NAME: &str = "flutter/navigation";

/// Plugin for the `flutter/navigation` channel.
#[derive(Default)]
pub struct NavigationPlugin {
    channel: Weak<MethodChannel>,
}

impl Plugin for NavigationPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        self.channel =
            engine.register_channel(MethodChannel::new(CHANNEL_NAME, Handler, &JSON_CODEC));
    }
}

impl NavigationPlugin {
    fn with_channel<F>(&self, f: F)
    where
        F: FnOnce(&MethodChannel),
    {
        if let Some(channel) = self.channel.upgrade() {
            f(&channel);
        }
    }

    /// Set the route the app starts on.
    ///
    /// Only has an effect before the framework's `WidgetsApp` builds its
    /// first route. Does nothing if the plugin was never registered, or if
    /// its engine has been dropped.
    pub fn set_initial_route(&self, initial_route: &str) {
        self.with_channel(|channel| {
            channel.invoke_method("setInitialRoute", initial_route.to_string())
        });
    }

    /// Push a route onto the framework's navigator.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped.
    pub fn push_route(&self, route: &str) {
        self.with_channel(|channel| channel.invoke_method("pushRoute", route.to_string()));
    }

    /// Pop the framework's topmost route.
    ///
    /// Does nothing if the plugin was never registered, or if its engine has
    /// been dropped.
    pub fn pop_route(&self) {
        self.with_channel(|channel| channel.invoke_method("popRoute", Value::Null));
    }
}

struct Handler;

impl MethodCallHandler for Handler {
    fn on_method_call(&mut self, call: MethodCall) {
        debug!(
            "got method call {} with args {:?}",
            call.method(),
            call.raw_args()
        );
        call.not_implemented()
    }
}
