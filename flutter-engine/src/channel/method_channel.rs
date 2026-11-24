//! A channel that carries method calls and their replies, Dart's
//! `MethodChannel`.

use log::error;

use crate::channel::Channel;
use crate::channel::platform_message::{PlatformMessage, PlatformMessageResponseHandle};
use crate::{FlutterEngine, FlutterEngineWeakRef, codec, codec::MethodCodec};

use crate::codec::value::{from_value, from_value_owned, to_value};
use crate::codec::{MethodCallResult, Value};
use crate::error::ValueError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt;

/// A method call Dart made on a [`MethodChannel`].
///
/// Dart is usually waiting for a reply: answer with [`success`](Self::success),
/// [`error`](Self::error) or [`not_implemented`](Self::not_implemented), each of
/// which consumes the call.
pub struct MethodCall {
    engine: FlutterEngineWeakRef,
    codec: &'static dyn MethodCodec,
    inner: codec::MethodCall,
    response_handle: Option<PlatformMessageResponseHandle>,
}

impl fmt::Debug for MethodCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MethodCall")
            .field("method", &self.inner.method)
            .field("args", &self.inner.args)
            .field("can_respond", &self.can_respond())
            .finish_non_exhaustive()
    }
}

/// The error half of a method call's result.
///
/// Deliberately *not* `#[non_exhaustive]`: these are the two failures the Dart
/// protocol defines, and a caller waiting for a reply has to handle both.
#[derive(Debug)]
pub enum MethodError<D>
where
    D: DeserializeOwned + Serialize,
{
    /// A Dart-visible error, which becomes a `PlatformException`.
    Err {
        /// Dart-visible error code.
        code: String,
        /// Human-readable message.
        message: String,
        /// Structured details handed back to Dart.
        details: D,
    },
    /// Nothing handled the method, which becomes a `MissingPluginException`.
    NotImplemented,
}

impl MethodCall {
    /// Deserialize the call arguments.
    ///
    /// # Errors
    ///
    /// Fails if the arguments do not have the shape `T` expects — which is what
    /// a Dart caller passing the wrong thing looks like, so it must be handled
    /// rather than unwrapped. Answer such a call with
    /// [`error`](Self::error).
    pub fn args<'a, T>(&'a self) -> Result<T, ValueError>
    where
        T: Deserialize<'a>,
    {
        from_value(&self.inner.args)
    }

    /// The call arguments as they were decoded.
    #[must_use]
    pub fn raw_args(&self) -> &Value {
        &self.inner.args
    }

    /// The method name.
    #[must_use]
    pub fn method(&self) -> &str {
        self.inner.method.as_str()
    }

    /// `true` if Dart is waiting for a reply.
    #[must_use]
    pub fn can_respond(&self) -> bool {
        self.response_handle.is_some()
    }

    /// Reply to the call.
    ///
    /// A call Dart did not ask a reply for is logged and dropped: check with
    /// [`can_respond`](Self::can_respond) first to avoid the log line.
    pub fn respond<T, D>(self, result: Result<T, MethodError<D>>)
    where
        T: Serialize,
        D: Serialize + DeserializeOwned,
    {
        let Some(engine) = self.engine.upgrade() else {
            error!("cannot respond on a channel whose engine is gone");
            return;
        };
        let Some(handle) = self.response_handle else {
            error!("respond() called on a method call Dart is not waiting for a reply on");
            return;
        };

        // Every path below has to produce *some* envelope: an unanswered
        // response handle leaks the memory behind it inside the engine.
        let result = match result {
            Ok(value) => match to_value(value) {
                Ok(value) => MethodCallResult::Ok(value),
                Err(error) => encode_failed(&error),
            },
            Err(MethodError::Err {
                code,
                message,
                details,
            }) => match to_value(details) {
                Ok(details) => MethodCallResult::Err {
                    code,
                    message,
                    details,
                },
                Err(error) => encode_failed(&error),
            },
            Err(MethodError::NotImplemented) => MethodCallResult::NotImplemented,
        };

        let buf = self.codec.encode_method_call_response(&result);
        engine.run_on_platform_thread(move |eng| {
            eng.send_platform_message_response(handle, &buf);
        });
    }

    /// Reply with a successful result.
    pub fn success<T>(self, data: T)
    where
        T: Serialize,
    {
        self.respond::<T, Value>(Ok(data))
    }

    /// Reply with a successful, empty result.
    pub fn success_empty(self) {
        self.respond::<Value, Value>(Ok(Value::Null))
    }

    /// Reply with a Dart-visible error.
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

    /// Reply that nothing handles this method.
    pub fn not_implemented(self) {
        self.respond::<Value, Value>(Err(MethodError::NotImplemented))
    }

    /// A weak handle to the engine this call arrived on.
    #[must_use]
    pub fn engine(&self) -> FlutterEngineWeakRef {
        self.engine.clone()
    }
}

/// Turn a reply that cannot be encoded into one that can.
fn encode_failed(error: &crate::codec::value::Error) -> MethodCallResult {
    error!("failed to encode a method call reply: {error}");
    MethodCallResult::Err {
        code: "encode-failed".to_owned(),
        message: error.to_string(),
        details: Value::Null,
    }
}

/// Handles the method calls Dart makes on one [`MethodChannel`].
pub trait MethodCallHandler {
    /// Called on the platform thread for every call on the channel.
    fn on_method_call(&mut self, call: MethodCall);
}

/// A channel that carries method calls, Dart's `MethodChannel`.
pub struct MethodChannel {
    name: String,
    engine: FlutterEngineWeakRef,
    method_handler: RefCell<Box<dyn MethodCallHandler>>,
    codec: &'static dyn MethodCodec,
}

impl fmt::Debug for MethodChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MethodChannel")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl MethodChannel {
    /// A channel named `name` that hands its calls to `method_handler`.
    ///
    /// Register it with
    /// [`FlutterEngine::register_channel`](crate::FlutterEngine::register_channel)
    /// to make it live.
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

    /// Call a method on the Dart side of this channel, ignoring the reply.
    ///
    /// `args` is encoded on the calling thread and the send itself hops onto
    /// the platform thread, which is the only thread the embedder API accepts
    /// messages from. Already being on it — as a plugin handler is — keeps the
    /// send synchronous, and so keeps it ordered against other sends.
    pub fn invoke_method<S, T>(&self, method: S, args: T)
    where
        S: Into<String>,
        T: Serialize,
    {
        if let Some(engine) = self.engine() {
            let method = method.into();
            let args = match to_value(args) {
                Ok(args) => args,
                Err(error) => {
                    error!("failed to encode the arguments of {method}: {error}");
                    return;
                }
            };
            let buf = self
                .codec()
                .encode_method_call(&codec::MethodCall { method, args });
            let name = self.name.clone();

            engine.run_on_platform_thread(move |eng| {
                eng.post_message(&name, &buf);
            });
        }
    }

    /// Call a method on the Dart side of this channel and hand the reply to
    /// `callback`.
    ///
    /// `callback` runs on the platform thread. It is not called at all if the
    /// reply envelope cannot be decoded, which is logged instead, or if the
    /// reply never comes — the send failed, or the engine was shut down with
    /// the message in flight — in which case it is dropped.
    pub fn invoke_method_with_result<T, F, V, D>(&self, method: String, args: T, callback: F)
    where
        T: Serialize,
        F: FnOnce(Result<V, MethodError<D>>) + 'static + Send,
        V: DeserializeOwned,
        D: DeserializeOwned + Serialize,
    {
        if let Some(engine) = self.engine() {
            let codec = self.codec;
            let args = match to_value(args) {
                Ok(args) => args,
                Err(error) => {
                    error!("failed to encode the arguments of {method}: {error}");
                    return;
                }
            };
            let buf = codec.encode_method_call(&codec::MethodCall { method, args });
            let name = self.name.clone();
            let reply_name = self.name.clone();

            engine.run_on_platform_thread(move |eng| {
                let Some(handle) = PlatformMessageResponseHandle::new(eng, move |data| {
                    let result = match codec.decode_envelope(data) {
                        Ok(result) => result,
                        Err(error) => {
                            error!("failed to decode a reply on channel {reply_name}: {error}");
                            return;
                        }
                    };

                    let response = match result {
                        MethodCallResult::Ok(value) => match from_value_owned(&value) {
                            Ok(value) => Ok(value),
                            Err(error) => {
                                error!(
                                    "failed to deserialize the result of a call on channel {reply_name}: {error}"
                                );
                                return;
                            }
                        },
                        MethodCallResult::Err {
                            code,
                            message,
                            details,
                        } => match from_value_owned(&details) {
                            Ok(details) => Err(MethodError::Err {
                                code,
                                message,
                                details,
                            }),
                            Err(error) => {
                                error!(
                                    "failed to deserialize the error details of a call on channel {reply_name}: {error}"
                                );
                                return;
                            }
                        },
                        MethodCallResult::NotImplemented => Err(MethodError::NotImplemented),
                    };

                    callback(response);
                }) else {
                    return;
                };

                eng.send_platform_message(PlatformMessage {
                    channel: Cow::Borrowed(&name),
                    message: &buf,
                    response_handle: Some(handle),
                });
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
        let call = match codec.decode_method_call(msg.message) {
            Ok(call) => call,
            Err(error) => {
                error!(
                    "failed to decode a method call on channel {}: {error}",
                    self.name
                );
                // Answer anyway: an unanswered handle leaks engine memory, and
                // an empty reply is what the framework reads as "no plugin
                // implements this".
                if let Some(handle) = msg.response_handle {
                    self.send_response(handle, &[]);
                }
                return;
            }
        };
        log::trace!(
            "on channel {}, got method call {} with args {:?}",
            self.name,
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
