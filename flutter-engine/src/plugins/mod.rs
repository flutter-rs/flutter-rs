use serde_json::Value;
use super::{ffi, FlutterEngineInner};
use std::{
    ptr::null,
    mem,
    sync::Weak,
    collections::HashMap,
    ffi::CString,
};

pub mod platform;
pub mod textinput;

pub struct PluginRegistry {
    map: HashMap<String, Vec<Box<dyn Plugin>>>,
    engine: Weak<FlutterEngineInner>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        PluginRegistry {
            map: HashMap::new(),
            engine: Weak::new(),
        }
    }
    pub fn set_engine(&mut self, engine: Weak<FlutterEngineInner>) {
        self.engine = engine;
    }
    pub fn add_plugin(&mut self, plugin: Box<dyn Plugin>) {
        let r = self.map.entry(plugin.get_channel()).or_insert_with(|| Vec::new());

        r.push(plugin);
    }
    pub fn handle(&mut self, msg: PlatformMessage, engine: &FlutterEngineInner, window: &mut glfw::Window) {
        for (channel, plugin) in &mut self.map {
            if channel == &msg.channel {
                for h in plugin {
                    h.handle(&msg, engine, window);
                }
            }
        }
    }
    pub fn get_plugin(&self, channel: &str) -> Option<&Box<dyn Plugin>> {
        if let Some(v) = self.map.get(channel) {
            Some(&v[0])
        } else {
            None
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub method: String,
    pub args: Value,
}

#[derive(Debug)]
pub struct PlatformMessage {
    pub channel: String,
    pub message: Message,
    pub response_handle: Option<&'static ffi::FlutterPlatformMessageResponseHandle>,
}

impl Into<ffi::FlutterPlatformMessage> for &PlatformMessage {
    fn into(self) -> ffi::FlutterPlatformMessage {
        let s = serde_json::to_string(&self.message).unwrap();
        let channel = CString::new(self.channel.to_owned()).unwrap();
        let message = s.into_bytes();
        let message_ptr = message.as_ptr();
        let message_len = message.len();

        mem::forget(message);
        // TODO: must manually clean up FlutterPlatformMessage

        let response_handle = if let Some(h) = self.response_handle {
            h as *const ffi::FlutterPlatformMessageResponseHandle
        } else {
            null()
        };

        ffi::FlutterPlatformMessage {
            struct_size: mem::size_of::<ffi::FlutterPlatformMessage>(),
            channel: channel.into_raw(),
            message: message_ptr,
            message_size: message_len,
            response_handle,
        }            
    }
}

pub trait Plugin {
    fn get_channel(&self) -> String;
    fn handle(&mut self, &PlatformMessage, &super::FlutterEngineInner, &mut glfw::Window) {}
    fn notify_changes(&self) {}
    fn set_registry(&mut self, _registry: *const PluginRegistry) {}
}
