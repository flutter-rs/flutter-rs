//! Register plugin with this registry to listen to flutter MethodChannel calls.

pub mod platform;
pub mod textinput;
pub mod dialog;
pub mod window;
pub mod navigation;

use super::{FlutterEngineInner};
use flutter_engine_sys::{
    FlutterPlatformMessageResponseHandle,
    FlutterPlatformMessage,
};
use std::{
    ptr::null,
    mem,
    sync::Weak,
    collections::HashMap,
    ffi::CString,
    borrow::Cow,
    sync::Arc,
};

pub struct PluginRegistry {
    map: HashMap<String, Box<dyn Plugin>>,
    pub engine: Weak<FlutterEngineInner>,
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
        let name = {
            let name = plugin.init_channel(self as &PluginRegistry);
            name.to_owned()
        };
        self.map.insert(name, plugin);
    }
    pub fn handle(&mut self, msg: PlatformMessage, engine: Arc<FlutterEngineInner>, window: &mut glfw::Window) {
        let mut message_handled = false;
        for (channel, plugin) in &mut self.map {
            if channel == &msg.channel {
                info!("Processing message from channel: {}", channel);
                plugin.handle(&msg, engine.clone(), window);
                message_handled = true;
            }
        }
        if !message_handled {
            warn!("No plugin registered to handle messages from channel: {}", &msg.channel);
        }
    }

    pub fn get_plugin(&self, channel: &str) -> Option<&Box<dyn Plugin>> {
        self.map.get(channel)
    }

    pub fn get_plugin_mut(&mut self, channel: &str) -> Option<&mut Box<dyn Plugin>> {
        self.map.get_mut(channel)
    }
}

#[derive(Debug)]
pub struct PlatformMessage<'a, 'b> {
    pub channel: Cow<'a, str>,
    pub message: &'b [u8],
    pub response_handle: Option<&'a FlutterPlatformMessageResponseHandle>,
}

impl<'a, 'b> PlatformMessage<'a, 'b> {
    fn get_response_handle(&self) -> Option<usize> {
        self.response_handle.map(|r| {
            r as *const FlutterPlatformMessageResponseHandle as usize
        })
    }
}

impl<'a, 'b> Into<FlutterPlatformMessage> for &PlatformMessage<'a, 'b> {
    fn into(self) -> FlutterPlatformMessage {
        let channel = CString::new(&*self.channel).unwrap();
        let message_ptr = self.message.as_ptr();
        let message_len = self.message.len();

        let response_handle = if let Some(h) = self.response_handle {
            h as *const FlutterPlatformMessageResponseHandle
        } else {
            null()
        };

        FlutterPlatformMessage {
            struct_size: mem::size_of::<FlutterPlatformMessage>(),
            channel: channel.into_raw(),
            message: message_ptr,
            message_size: message_len,
            response_handle,
        }
    }
}

pub trait Plugin {
    fn init_channel(&self, &PluginRegistry) -> &str;
    fn handle(&mut self, msg: &PlatformMessage, engine: Arc<FlutterEngineInner>, window: &mut glfw::Window);
}
