//! Plugins use MethodChannel to interop with flutter/dart.
//! It contains two implementations StandardMethodChannel using binary encoding
//! and JsonMethodChannel using json encoding.

pub use self::{
    event_channel::EventChannel, json_method_channel::JsonMethodChannel,
    registrar::ChannelRegistrar, standard_method_channel::StandardMethodChannel,
};
use crate::{
    codec::{MethodCall, MethodCallResult, MethodCodec, Value},
    desktop_window_state::RuntimeData,
    error::MethodCallError,
    ffi::{FlutterEngine, PlatformMessage, PlatformMessageResponseHandle},
    Window,
};

use std::{
    borrow::Cow,
    sync::{Arc, RwLock, Weak},
};

use log::error;

mod event_channel;
mod json_method_channel;
mod registrar;
mod standard_method_channel;

pub trait Channel {
    fn name(&self) -> &str;
    fn engine(&self) -> Option<Arc<FlutterEngine>>;
    fn init(&mut self, runtime_data: Weak<RuntimeData>);
    fn method_handler(&self) -> Option<Arc<RwLock<MethodCallHandler>>>;
    fn codec(&self) -> &MethodCodec;

    /// Handle a method call received on this channel
    fn handle_method(&self, msg: &mut PlatformMessage, window: &mut Window) {
        if let Some(handler) = self.method_handler() {
            let mut handler = handler.write().unwrap();
            let call = self.decode_method_call(&msg).unwrap();
            let method = call.method.clone();
            let result = handler.on_method_call(call, window);
            let response = match result {
                Ok(value) => MethodCallResult::Ok(value),
                Err(error) => {
                    error!(target: handler.module_path(), "error in method call {}#{}: {}", msg.channel, method, error);
                    error.into()
                }
            };
            self.send_method_call_response(&mut msg.response_handle, response);
        }
    }

    /// Invoke a flutter method using this channel
    fn invoke_method(&self, method_call: MethodCall) {
        let buf = self.codec().encode_method_call(&method_call);
        self.send_platform_message(PlatformMessage {
            channel: Cow::Borrowed(self.name()),
            message: &buf,
            response_handle: None,
        });
    }

    /// Decode dart method call
    fn decode_method_call(&self, msg: &PlatformMessage) -> Option<MethodCall> {
        self.codec().decode_method_call(msg.message)
    }

    /// When flutter listen to a stream of events using EventChannel.
    /// This method send back a success event.
    /// It can be call multiple times to simulate stream.
    fn send_success_event(&self, data: &Value) {
        let buf = self.codec().encode_success_envelope(data);
        self.send_platform_message(PlatformMessage {
            channel: Cow::Borrowed(self.name()),
            message: &buf,
            response_handle: None,
        });
    }

    /// When flutter listen to a stream of events using EventChannel.
    /// This method send back a error event.
    /// It can be call multiple times to simulate stream.
    fn send_error_event(&self, code: &str, message: &str, data: &Value) {
        let buf = self.codec().encode_error_envelope(code, message, data);
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
        response: MethodCallResult,
    ) {
        if let Some(response_handle) = response_handle.take() {
            let buf = match response {
                MethodCallResult::Ok(data) => self.codec().encode_success_envelope(&data),
                MethodCallResult::Err {
                    code,
                    message,
                    details,
                } => self
                    .codec()
                    .encode_error_envelope(&code, &message, &details),
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

pub trait MethodCallHandler {
    fn module_path(&self) -> &'static str {
        module_path!()
    }

    fn on_method_call(
        &mut self,
        call: MethodCall,
        window: &mut Window,
    ) -> Result<Value, MethodCallError>;
}
