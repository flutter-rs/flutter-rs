//! This plugin is used for navigation in an app.
//! It handles flutter/navigation type messages.

use std::{
    cell::RefCell,
    sync::Arc,
};
use glfw::{Modifiers};
use crate::FlutterEngineInner;
use super::{ Plugin, PlatformMessage, PluginRegistry};
use codec::{ MethodCall };
use serde_json::Value;
use utils::StringUtils;
use channel::{ Channel, JsonMethodChannel };

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
            args: crate::codec::json_codec::Value::Null,
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
