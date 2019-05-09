//! This plugin is used for navigation in an app.
//! It handles flutter/navigation type messages.

use super::prelude::*;

use log::debug;

pub const PLUGIN_NAME: &str = "flutter-engine::plugins::navigation";
pub const CHANNEL_NAME: &str = "flutter/navigation";

pub struct NavigationPlugin {
    channel: Weak<JsonMethodChannel>,
    handler: Arc<RwLock<Handler>>,
}

impl Plugin for NavigationPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, registrar: &mut ChannelRegistrar) {
        let method_handler = Arc::downgrade(&self.handler);
        self.channel =
            registrar.register_channel(JsonMethodChannel::new(CHANNEL_NAME, method_handler));
    }
}

impl NavigationPlugin {
    pub fn new() -> Self {
        Self {
            channel: Weak::new(),
            handler: Arc::new(RwLock::new(Handler)),
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

struct Handler;

impl MethodCallHandler for Handler {
    fn on_method_call(
        &mut self,
        call: MethodCall,
        _: RuntimeData,
    ) -> Result<Value, MethodCallError> {
        debug!("got method call {} with args {:?}", call.method, call.args);
        Err(MethodCallError::NotImplemented)
    }
}
