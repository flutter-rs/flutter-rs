//! This plugin is used for navigation in an app.
//! It handles flutter/navigation type messages.

use super::{PlatformMessage, Plugin, PluginChannel};
use crate::{
    channel::{Channel, JsonMethodChannel},
    codec::MethodCall,
    desktop_window_state::RuntimeData,
};

use std::sync::Weak;

use log::info;
use serde_json::{json, Value};

pub const CHANNEL_NAME: &str = "flutter/navigation";

pub struct NavigationPlugin {
    channel: JsonMethodChannel,
}

impl PluginChannel for NavigationPlugin {
    fn channel_name() -> &'static str {
        CHANNEL_NAME
    }
}

impl NavigationPlugin {
    pub fn new() -> Self {
        Self {
            channel: JsonMethodChannel::new(CHANNEL_NAME),
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
    fn init_channel(&mut self, runtime_data: Weak<RuntimeData>) {
        self.channel.init(runtime_data);
    }

    fn handle(&mut self, msg: &PlatformMessage, _: &mut glfw::Window) {
        let decoded = self.channel.decode_method_call(msg).unwrap();

        info!(
            "navigation method {:?} called with args {:?}",
            decoded.method, decoded.args
        );
    }
}
