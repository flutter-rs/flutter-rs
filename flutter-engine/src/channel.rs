use plugins::PluginRegistry;
use plugins::PlatformMessage;
use codec::{ MethodCodec, MethodCall, MethodCallResult, json_codec, standard_codec };
use std::sync::Weak;
use crate::FlutterEngineInner;
use std::borrow::Cow;
use std::cell::RefCell;
use ffi;

pub trait Channel {
    fn get_name(&self) -> &str;
    fn init(&self, registry: *const PluginRegistry);
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

    pub fn get_engine(&self) -> Option<Weak<FlutterEngineInner>> {
        self.registry.borrow().map(|ptr| {
            unsafe {
                let registry = &*ptr;
                registry.engine.clone()
            }
        })
    }

    pub fn invoke_method(&self, method_call: MethodCall<json_codec::Value>) {
        let buf = json_codec::JsonMethodCodec::encode_method_call(&method_call);
        self.send_platform_message(&PlatformMessage {
            channel: Cow::Borrowed(&self.name),
            message: &buf,
            response_handle: None,
        });
    }

    pub fn decode_method_call(&self, msg: &PlatformMessage) -> MethodCall<json_codec::Value> {
        json_codec::JsonMethodCodec::decode_method_call(msg.message).unwrap()
    }

    pub fn send_success_event(&self, data: &json_codec::Value) {
        let buf = json_codec::JsonMethodCodec::encode_success_envelope(data);
        self.send_platform_message(&PlatformMessage {
            channel: Cow::Borrowed(&self.name),
            message: &buf,
            response_handle: None,
        });
    }
    
    pub fn send_error_event(&self, code: &str, message: &str, data: &json_codec::Value) {
        let buf = json_codec::JsonMethodCodec::encode_error_envelope(code, message, data);
        self.send_platform_message(&PlatformMessage {
            channel: Cow::Borrowed(&self.name),
            message: &buf,
            response_handle: None,
        });
    }

    pub fn send_platform_message(&self, msg: &PlatformMessage) {
        if let Some(engine) = self.get_engine() {
            engine.upgrade().unwrap().send_platform_message(msg);
        } else {
            error!("Cannot get engine");
        }
    }

    pub fn send_method_call_response(&self, response_handle: Option<&ffi::FlutterPlatformMessageResponseHandle>, ret: MethodCallResult<json_codec::Value>) -> bool {
        if let Some(handle) = response_handle {
            let buf = match ret {
                MethodCallResult::Ok(data) => (
                    json_codec::JsonMethodCodec::encode_success_envelope(&data)
                ),
                MethodCallResult::Err{code, message, data} => (
                    json_codec::JsonMethodCodec::encode_error_envelope(&code, &message, &data)
                )
            };
            self.send_response(handle, &buf);
            true       
        } else {
            false
        }
    }

    pub fn send_response(&self, response_handle: &ffi::FlutterPlatformMessageResponseHandle, buf: &[u8]) {
        if let Some(engine) = self.get_engine() {
            engine.upgrade().unwrap().send_platform_message_response(
                response_handle,
                buf,
            );
        }
    }
}

impl Channel for JsonMethodChannel {
    fn get_name(&self) -> &str {
        &self.name
    }
    fn init(&self, registry: *const PluginRegistry) {
        self.registry.replace(Some(registry));
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

    pub fn get_engine(&self) -> Option<Weak<FlutterEngineInner>> {
        self.registry.borrow().map(|ptr| {
            unsafe {
                let registry = &*ptr;
                registry.engine.clone()
            }
        })
    }

    pub fn invoke_method(&self, method_call: MethodCall<standard_codec::Value>) {
        let buf = standard_codec::StandardMethodCodec::encode_method_call(&method_call);
        self.send_platform_message(&PlatformMessage {
            channel: Cow::Borrowed(&self.name),
            message: &buf,
            response_handle: None,
        });
    }

    pub fn decode_method_call(&self, msg: &PlatformMessage) -> MethodCall<standard_codec::Value> {
        standard_codec::StandardMethodCodec::decode_method_call(msg.message).unwrap()
    }

    pub fn send_success_event(&self, data: &standard_codec::Value) {
        let buf = standard_codec::StandardMethodCodec::encode_success_envelope(data);
        self.send_platform_message(&PlatformMessage {
            channel: Cow::Borrowed(&self.name),
            message: &buf,
            response_handle: None,
        });
    }
    
    pub fn send_error_event(&self, code: &str, message: &str, data: &standard_codec::Value) {
        let buf = standard_codec::StandardMethodCodec::encode_error_envelope(code, message, data);
        self.send_platform_message(&PlatformMessage {
            channel: Cow::Borrowed(&self.name),
            message: &buf,
            response_handle: None,
        });
    }    

    pub fn send_platform_message(&self, msg: &PlatformMessage) {
        if let Some(engine) = self.get_engine() {
            engine.upgrade().unwrap().send_platform_message(msg);
        } else {
            error!("Cannot get engine");
        }
    }

    pub fn send_method_call_response(&self, response_handle: Option<&ffi::FlutterPlatformMessageResponseHandle>, ret: MethodCallResult<standard_codec::Value>) -> bool {
        if let Some(handle) = response_handle {
            let buf = match ret {
                MethodCallResult::Ok(data) => (
                    standard_codec::StandardMethodCodec::encode_success_envelope(&data)
                ),
                MethodCallResult::Err{code, message, data} => (
                    standard_codec::StandardMethodCodec::encode_error_envelope(&code, &message, &data)
                )
            };
            self.send_response(handle, &buf);
            true
        } else {
            false
        }
    }

    pub fn send_response(&self, response_handle: &ffi::FlutterPlatformMessageResponseHandle, buf: &[u8]) {
        if let Some(engine) = self.get_engine() {
            engine.upgrade().unwrap().send_platform_message_response(
                response_handle,
                buf,
            );
        }
    }
}

impl Channel for StandardMethodChannel {
    fn get_name(&self) -> &str {
        &self.name
    }
    fn init(&self, registry: *const PluginRegistry) {
        self.registry.replace(Some(registry));
    }
}