use std::sync::{Arc, Weak};

use crate::{
    channel::Channel,
    codec::standard_codec::{StandardMethodCodec, Value},
    desktop_window_state::RuntimeData,
    ffi::FlutterEngine,
    PlatformMessage,
};

use crate::codec::MethodCallResult;
use log::{error, warn};

pub struct EventChannel<D> {
    name: String,
    engine: Weak<FlutterEngine>,
    listen_fn:
        Box<dyn Fn(&mut D, <Self as Channel>::R) -> MethodCallResult<<Self as Channel>::R> + Send>,
    cancel_fn: Box<dyn Fn(&mut D) -> MethodCallResult<<Self as Channel>::R> + Send>,
}

impl<D> EventChannel<D> {
    pub fn new<L, C>(name: &str, listen_fn: L, cancel_fn: C) -> Self
    where
        L: Fn(&mut D, <Self as Channel>::R) -> MethodCallResult<<Self as Channel>::R>
            + Send
            + 'static,
        C: Fn(&mut D) -> MethodCallResult<<Self as Channel>::R> + Send + 'static,
    {
        Self {
            name: name.to_owned(),
            engine: Weak::new(),
            listen_fn: Box::new(listen_fn),
            cancel_fn: Box::new(cancel_fn),
        }
    }

    pub fn handle(&self, data: &mut D, msg: &mut PlatformMessage) {
        let decoded = self.decode_method_call(msg).unwrap();
        match decoded.method.as_str() {
            "listen" => {
                let response = (self.listen_fn)(data, decoded.args);
                self.send_method_call_response(&mut msg.response_handle, response);
            }
            "cancel" => {
                let response = (self.cancel_fn)(data);
                self.send_method_call_response(&mut msg.response_handle, response);
            }
            method => {
                warn!(
                    "Unknown method {} called! Maybe this is not an event channel?",
                    method
                );
                self.send_method_call_response(
                    &mut msg.response_handle,
                    MethodCallResult::NotImplemented,
                );
            }
        }
    }
}

impl<D> Channel for EventChannel<D> {
    type R = Value;
    type Codec = StandardMethodCodec;

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
