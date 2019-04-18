//! Plugins use MethodChannel to interop with flutter/dart.
//! It contains two implementations StandardMethodChannel using binary encoding
//! and JsonMethodChannel using json encoding.

use crate::{
    codec::{json_codec, standard_codec, MethodCall, MethodCallResult, MethodCodec},
    desktop_window_state::RuntimeData,
    ffi::{FlutterEngine, PlatformMessage, PlatformMessageResponseHandle},
};

use std::{
    borrow::Cow,
    sync::{Arc, Weak},
};

use log::error;

pub trait Channel {
    type R;
    type Codec: MethodCodec<R = Self::R>;

    fn name(&self) -> &str;
    fn engine(&self) -> Option<Arc<FlutterEngine>>;
    fn init(&mut self, runtime_data: Weak<RuntimeData>);

    /// Invoke a flutter method using this channel
    fn invoke_method(&self, method_call: MethodCall<Self::R>) {
        let buf = Self::Codec::encode_method_call(&method_call);
        self.send_platform_message(PlatformMessage {
            channel: Cow::Borrowed(self.name()),
            message: &buf,
            response_handle: None,
        });
    }

    /// Decode dart method call
    fn decode_method_call(&self, msg: &PlatformMessage) -> Option<MethodCall<Self::R>> {
        Self::Codec::decode_method_call(msg.message)
    }

    /// When flutter listen to a stream of events using EventChannel.
    /// This method send back a success event.
    /// It can be call multiple times to simulate stream.
    fn send_success_event(&self, data: &Self::R) {
        let buf = Self::Codec::encode_success_envelope(data);
        self.send_platform_message(PlatformMessage {
            channel: Cow::Borrowed(self.name()),
            message: &buf,
            response_handle: None,
        });
    }

    /// When flutter listen to a stream of events using EventChannel.
    /// This method send back a error event.
    /// It can be call multiple times to simulate stream.
    fn send_error_event(&self, code: &str, message: &str, data: &Self::R) {
        let buf = Self::Codec::encode_error_envelope(code, message, data);
        self.send_platform_message(PlatformMessage {
            channel: Cow::Borrowed(self.name()),
            message: &buf,
            response_handle: None,
        });
    }

    /// Send a method call response
    fn send_method_call_response(
        &self,
        response_handle: &mut Option<PlatformMessageResponseHandle>,
        response: MethodCallResult<Self::R>,
    ) {
        if let Some(response_handle) = response_handle.take() {
            let buf = match response {
                MethodCallResult::Ok(data) => Self::Codec::encode_success_envelope(&data),
                MethodCallResult::Err {
                    code,
                    message,
                    details,
                } => Self::Codec::encode_error_envelope(&code, &message, &details),
                MethodCallResult::NotImplemented => vec![],
            };
            self.send_response(response_handle, &buf);
        }
    }

    /// When flutter call a method using MethodChannel,
    /// it can wait for rust response using await syntax.
    /// This method send a response to flutter. This is a low level method.
    /// Please use send_method_call_response if that will work.
    fn send_response(&self, response_handle: PlatformMessageResponseHandle, buf: &[u8]) {
        if let Some(engine) = self.engine() {
            engine.send_platform_message_response(response_handle, buf);
        } else {
            error!("Channel {} was not initialized", self.name());
        }
    }

    /// Send a platform message over this channel. This is a low level method.
    fn send_platform_message(&self, message: PlatformMessage) {
        if let Some(engine) = self.engine() {
            engine.send_platform_message(message);
        } else {
            error!("Channel {} was not initialized", self.name());
        }
    }
}

pub struct JsonMethodChannel {
    name: String,
    engine: Weak<FlutterEngine>,
}

impl JsonMethodChannel {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            engine: Weak::new(),
        }
    }
}

impl Channel for JsonMethodChannel {
    type R = json_codec::Value;
    type Codec = json_codec::JsonMethodCodec;

    fn name(&self) -> &str {
        &self.name
    }

    fn engine(&self) -> Option<Arc<FlutterEngine>> {
        self.engine.upgrade()
    }

    fn init(&mut self, runtime_data: Weak<RuntimeData>) {
        if self.engine.upgrade().is_some() {
            error!("Channel {} was already initialized", self.name);
        }
        if let Some(runtime_data) = runtime_data.upgrade() {
            self.engine = Arc::downgrade(&runtime_data.engine);
        }
    }
}

pub struct StandardMethodChannel {
    name: String,
    engine: Weak<FlutterEngine>,
}

impl StandardMethodChannel {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            engine: Weak::new(),
        }
    }
}

impl Channel for StandardMethodChannel {
    type R = standard_codec::Value;
    type Codec = standard_codec::StandardMethodCodec;

    fn name(&self) -> &str {
        &self.name
    }

    fn engine(&self) -> Option<Arc<FlutterEngine>> {
        self.engine.upgrade()
    }

    fn init(&mut self, runtime_data: Weak<RuntimeData>) {
        if self.engine.upgrade().is_some() {
            error!("Channel {} was already initialized", self.name);
        }
        if let Some(runtime_data) = runtime_data.upgrade() {
            self.engine = Arc::downgrade(&runtime_data.engine);
        }
    }
}
