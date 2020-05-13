use log::error;

use crate::channel::platform_message::{PlatformMessage, PlatformMessageResponseHandle};
use crate::channel::Channel;
use crate::{codec, codec::MethodCodec, FlutterEngine, FlutterEngineWeakRef};

use crate::codec::value::{from_value, from_value_owned, to_value};
use crate::codec::{MethodCallResult, Value};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;

pub struct MethodCall {
    engine: FlutterEngineWeakRef,
    codec: &'static dyn MethodCodec,
    inner: codec::MethodCall,
    response_handle: Option<PlatformMessageResponseHandle>,
}

pub enum MethodError<D>
where
    D: DeserializeOwned + Serialize,
{
    Err {
        code: String,
        message: String,
        details: D,
    },
    NotImplemented,
}

impl MethodCall {
    pub fn args<'a, T>(&'a self) -> T
    where
        T: Deserialize<'a>,
    {
        from_value(&self.inner.args).unwrap()
    }

    pub fn raw_args(&self) -> &Value {
        &self.inner.args
    }

    pub fn method(&self) -> &String {
        &self.inner.method
    }

    pub fn can_respond(&self) -> bool {
        self.response_handle.is_some()
    }

    pub fn respond<T, D>(self, result: Result<T, MethodError<D>>)
    where
        T: Serialize,
        D: Serialize + DeserializeOwned,
    {
        if let Some(engine) = self.engine.upgrade() {
            let handle = self
                .response_handle
                .expect("Message can not be response handle");

            let result = match result {
                Ok(val) => {
                    let value = to_value(val).expect("Failed to encode data to value");
                    MethodCallResult::Ok(value)
                }
                Err(err) => match err {
                    MethodError::Err {
                        code,
                        message,
                        details,
                    } => {
                        let details = to_value(details).expect("Failed to encode details to value");
                        MethodCallResult::Err {
                            code,
                            message,
                            details,
                        }
                    }
                    MethodError::NotImplemented => MethodCallResult::NotImplemented,
                },
            };

            let buf = self.codec.encode_method_call_response(&result);
            engine.run_on_platform_thread(move |eng| {
                eng.send_platform_message_response(handle, &buf);
            });
        }
    }

    pub fn success<T>(self, data: T)
    where
        T: Serialize,
    {
        self.respond::<T, Value>(Ok(data))
    }

    pub fn success_empty(self) {
        self.respond::<Value, Value>(Ok(Value::Null))
    }

    pub fn error<T, S1, S2>(self, code: S1, message: S2, details: T)
    where
        T: Serialize + DeserializeOwned,
        S1: Into<String>,
        S2: Into<String>,
    {
        self.respond::<Value, T>(Err(MethodError::Err {
            code: code.into(),
            message: message.into(),
            details,
        }))
    }

    pub fn not_implemented(self) {
        self.respond::<Value, Value>(Err(MethodError::NotImplemented))
    }

    pub fn engine(&self) -> FlutterEngineWeakRef {
        self.engine.clone()
    }
}

pub trait MethodCallHandler {
    fn on_method_call(&mut self, call: MethodCall);
}

pub struct MethodChannel {
    name: String,
    engine: FlutterEngineWeakRef,
    method_handler: RefCell<Box<dyn MethodCallHandler>>,
    codec: &'static dyn MethodCodec,
}

impl MethodChannel {
    pub fn new<N, H>(name: N, method_handler: H, codec: &'static dyn MethodCodec) -> Self
    where
        N: AsRef<str>,
        H: MethodCallHandler + 'static,
    {
        Self {
            name: name.as_ref().to_owned(),
            engine: Default::default(),
            method_handler: RefCell::new(Box::new(method_handler)),
            codec,
        }
    }

    fn codec(&self) -> &'static dyn MethodCodec {
        self.codec
    }

    /// Invoke a flutter method using this channel
    pub fn invoke_method<S, T>(&self, method: S, args: T)
    where
        S: Into<String>,
        T: Serialize,
    {
        if let Some(engine) = self.engine() {
            let buf = self.codec().encode_method_call(&codec::MethodCall {
                method: method.into(),
                args: to_value(args).expect("Failed to encode args to value"),
            });

            engine.send_platform_message(PlatformMessage {
                channel: Cow::Borrowed(self.name()),
                message: &buf,
                response_handle: None,
            });
        }
    }

    /// Invoke a flutter method using this channel
    pub fn invoke_method_with_result<T, F, V, D>(&self, method: String, args: T, callback: F)
    where
        T: Serialize,
        F: FnOnce(Result<V, MethodError<D>>) -> () + 'static + Send,
        V: DeserializeOwned,
        D: DeserializeOwned + Serialize,
    {
        if let Some(engine) = self.engine() {
            let codec = self.codec;

            let buf = codec.encode_method_call(&codec::MethodCall {
                method,
                args: to_value(args).expect("Failed to encode args to value"),
            });

            let handle = PlatformMessageResponseHandle::new(engine.clone(), move |data| {
                let result = codec
                    .decode_envelope(data)
                    .expect("Failed to decode response envelope");

                let response = match result {
                    MethodCallResult::Ok(val) => {
                        Ok(from_value_owned(&val).expect("Failed to decode success response"))
                    }
                    MethodCallResult::Err {
                        code,
                        message,
                        details,
                    } => Err(MethodError::Err {
                        code,
                        message,
                        details: from_value_owned(&details)
                            .expect("Failed to deserialize error details"),
                    }),
                    MethodCallResult::NotImplemented => Err(MethodError::NotImplemented),
                };

                callback(response);
            });

            engine.send_platform_message(PlatformMessage {
                channel: Cow::Borrowed(self.name()),
                message: &buf,
                response_handle: Some(handle),
            });
        }
    }
}

impl Channel for MethodChannel {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn engine(&self) -> Option<FlutterEngine> {
        self.engine.upgrade()
    }

    fn init(&mut self, engine: FlutterEngineWeakRef) {
        if self.engine.upgrade().is_some() {
            error!("Channel {} was already initialized", self.name);
        }
        self.engine = engine;
    }

    /// Handle incoming message received on this channel
    fn handle_platform_message(&self, msg: PlatformMessage) {
        debug_assert_eq!(msg.channel, self.name());
        let codec = self.codec;
        let call = self.codec.decode_method_call(msg.message).unwrap();
        let channel = self.name().to_owned();
        log::trace!(
            "on channel {}, got method call {} with args {:?}",
            channel,
            call.method,
            call.args
        );

        let call = MethodCall {
            engine: self.engine.clone(),
            codec,
            inner: call,
            response_handle: msg.response_handle,
        };

        self.method_handler.borrow_mut().on_method_call(call);
    }
}
