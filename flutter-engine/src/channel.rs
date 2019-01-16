use std::{
    sync::Weak,
    borrow::Cow,
    cell::RefCell,
};
use crate::{
    FlutterEngineInner,
    codec::{
        MethodCodec,
        MethodCall,
        MethodCallResult,
        json_codec,
        standard_codec
    },
    plugins::{ PluginRegistry, PlatformMessage},
};
use ffi;

pub trait Channel {
    type R;
    type Codec: MethodCodec<R=Self::R>;

    fn get_name(&self) -> &str;
    fn init(&self, registry: *const PluginRegistry);
    fn get_engine(&self) -> Option<Weak<FlutterEngineInner>>;

    /// Invoke a dart method using this channel
    fn invoke_method(&self, method_call: MethodCall<Self::R>) {
        let buf = Self::Codec::encode_method_call(&method_call);
        self.send_platform_message(&PlatformMessage {
            channel: Cow::Borrowed(self.get_name()),
            message: &buf,
            response_handle: None,
        });
    }

    /// Decode dart method call
    fn decode_method_call(&self, msg: &PlatformMessage) -> MethodCall<Self::R> {
        Self::Codec::decode_method_call(msg.message).unwrap()
    }

    /// Send an EventChannel success event
    fn send_success_event(&self, data: &Self::R) {
        let buf = Self::Codec::encode_success_envelope(data);
        self.send_platform_message(&PlatformMessage {
            channel: Cow::Borrowed(self.get_name()),
            message: &buf,
            response_handle: None,
        });
    }

    /// Send an EventChannel error event
    fn send_error_event(&self, code: &str, message: &str, data: &Self::R) {
        let buf = Self::Codec::encode_error_envelope(code, message, data);
        self.send_platform_message(&PlatformMessage {
            channel: Cow::Borrowed(self.get_name()),
            message: &buf,
            response_handle: None,
        });
    }

    /// Send a method call response
    fn send_method_call_response(&self, response_handle: Option<&ffi::FlutterPlatformMessageResponseHandle>, ret: MethodCallResult<Self::R>) -> bool {
        if let Some(handle) = response_handle {
            let buf = match ret {
                MethodCallResult::Ok(data) => (
                    Self::Codec::encode_success_envelope(&data)
                ),
                MethodCallResult::Err{code, message, details} => (
                    Self::Codec::encode_error_envelope(&code, &message, &details)
                )
            };
            self.send_response(handle, &buf);
            true       
        } else {
            false
        }
    }

    /// Send a method call response. This is a low level method. Please use send_method_call_response
    fn send_response(&self, response_handle: &ffi::FlutterPlatformMessageResponseHandle, buf: &[u8]) {
        if let Some(engine) = self.get_engine() {
            engine.upgrade().unwrap().send_platform_message_response(
                response_handle,
                buf,
            );
        }
    }

    /// Send platform message
    fn send_platform_message(&self, msg: &PlatformMessage) {
        if let Some(engine) = self.get_engine() {
            engine.upgrade().unwrap().send_platform_message(msg);
        } else {
            error!("Cannot get engine");
        }
    }
}

pub struct JsonMethodChannel {
    name: String,
    registry: RefCell<Option<*const PluginRegistry>>,
}

unsafe impl Send for JsonMethodChannel {}
unsafe impl Sync for JsonMethodChannel {}

impl JsonMethodChannel {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            registry: RefCell::new(None),
        }
    }
}

impl Channel for JsonMethodChannel {
    type Codec = json_codec::JsonMethodCodec;
    type R = json_codec::Value;

    fn get_name(&self) -> &str {
        &self.name
    }
    fn init(&self, registry: *const PluginRegistry) {
        self.registry.replace(Some(registry));
    }

    fn get_engine(&self) -> Option<Weak<FlutterEngineInner>> {
        self.registry.borrow().map(|ptr| {
            unsafe {
                let registry = &*ptr;
                registry.engine.clone()
            }
        })
    }
}


pub struct StandardMethodChannel {
    name: String,
    registry: RefCell<Option<*const PluginRegistry>>,
}

unsafe impl Send for StandardMethodChannel {}
unsafe impl Sync for StandardMethodChannel {}

impl StandardMethodChannel {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            registry: RefCell::new(None),
        }
    }
}

impl Channel for StandardMethodChannel {
    type Codec = standard_codec::StandardMethodCodec;
    type R = standard_codec::Value;

    fn get_name(&self) -> &str {
        &self.name
    }
    fn init(&self, registry: *const PluginRegistry) {
        self.registry.replace(Some(registry));
    }
    fn get_engine(&self) -> Option<Weak<FlutterEngineInner>> {
        self.registry.borrow().map(|ptr| {
            unsafe {
                let registry = &*ptr;
                registry.engine.clone()
            }
        })
    }
}