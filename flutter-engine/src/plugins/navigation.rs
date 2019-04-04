//! This plugin is used for navigation in an app.
//! It handles flutter/navigation type messages.

use std::sync::Arc;

use crate::{FlutterEngineInner, codec::MethodCall, channel::{ Channel, JsonMethodChannel }};
use super::{PlatformMessage, Plugin, PluginRegistry};

use serde_json::Value;

pub struct NavigationPlugin {
    channel: JsonMethodChannel,
}

impl NavigationPlugin {
    pub fn new() -> Self {
        Self {
            channel: JsonMethodChannel::new("flutter/navigation"),
        }
    }

    pub fn set_initial_route(&self, initial_route: &str) {
        self.channel.invoke_method(MethodCall {
            method: String::from("setInitialRoute"),
            args: json!(initial_route),
        });
    }

    pub fn push_route(&self, route: &str) {
        self.channel.invoke_method(MethodCall {
            method: String::from("pushRoute"),
            args: json!(route),
        });
    }

    pub fn pop_route(&self) {
        self.channel.invoke_method(MethodCall {
            method: String::from("popRoute"),
            args: Value::Null,
        });
    }
}

impl Plugin for NavigationPlugin {
    fn init_channel(&self, registry: &PluginRegistry) -> &str {
        self.channel.init(registry);
        self.channel.get_name()
    }

    fn handle(&mut self, msg: &PlatformMessage, _: Arc<FlutterEngineInner>, _: &mut glfw::Window) {
        let decoded = self.channel.decode_method_call(msg);

        debug!("navigation methoid {:?} called with args {:?}", decoded.method, decoded.args);
    }
}
