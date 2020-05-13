//! This plugin is used for navigation in an app.
//! It handles flutter/navigation type messages.

use log::debug;
use std::sync::Weak;

use flutter_engine::channel::MethodCall;
use flutter_engine::codec::Value;
use flutter_engine::{
    channel::{MethodCallHandler, MethodChannel},
    codec::JSON_CODEC,
    plugins::Plugin,
    FlutterEngine,
};

pub const PLUGIN_NAME: &str = module_path!();
pub const CHANNEL_NAME: &str = "flutter/navigation";

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

impl Default for NavigationPlugin {
    fn default() -> Self {
        Self {
            channel: Weak::new(),
        }
    }
}

impl NavigationPlugin {
    fn with_channel<F>(&self, f: F)
    where
        F: FnOnce(&MethodChannel),
    {
        if let Some(channel) = self.channel.upgrade() {
            f(&*channel);
        }
    }

    pub fn set_initial_route(&self, initial_route: &str) {
        self.with_channel(|channel| {
            channel.invoke_method("setInitialRoute", initial_route.to_string())
        });
    }

    pub fn push_route(&self, route: &str) {
        self.with_channel(|channel| channel.invoke_method("pushRoute", route.to_string()));
    }

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
