//! This plugin is used for navigation in an app.
//! It handles flutter/navigation type messages.

use super::prelude::*;

use log::info;

pub const PLUGIN_NAME: &str = "flutter-engine::plugins::navigation";
pub const CHANNEL_NAME: &str = "flutter/navigation";

#[derive(Default)]
pub struct NavigationPlugin {
    channel: Weak<JsonMethodChannel>,
}

impl Plugin for NavigationPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, plugin: Weak<RwLock<Self>>, registrar: &mut ChannelRegistrar) {
        self.channel = registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, plugin));
    }
}

impl NavigationPlugin {
    pub fn new() -> Self {
        Self {
            channel: Weak::new(),
        }
    }

    fn with_channel<F>(&self, f: F)
    where
        F: FnOnce(&Channel),
    {
        if let Some(channel) = self.channel.upgrade() {
            f(&*channel);
        }
    }

    pub fn set_initial_route(&self, initial_route: &str) {
        self.with_channel(|channel| {
            channel.invoke_method(MethodCall {
                method: String::from("setInitialRoute"),
                args: Value::String(initial_route.into()),
            })
        });
    }

    pub fn push_route(&self, route: &str) {
        self.with_channel(|channel| {
            channel.invoke_method(MethodCall {
                method: String::from("pushRoute"),
                args: Value::String(route.into()),
            })
        });
    }

    pub fn pop_route(&self) {
        self.with_channel(|channel| {
            channel.invoke_method(MethodCall {
                method: String::from("popRoute"),
                args: Value::Null,
            })
        });
    }
}

impl MethodCallHandler for NavigationPlugin {
    fn on_method_call(
        &mut self,
        _: &str,
        call: MethodCall,
        _: &mut Window,
    ) -> Result<Value, MethodCallError> {
        info!(
            "navigation method {:?} called with args {:?}",
            call.method, call.args
        );
        Err(MethodCallError::NotImplemented)
    }
}
