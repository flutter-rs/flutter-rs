//! A channel that carries plain values in both directions, Dart's
//! `BasicMessageChannel`.

use crate::{FlutterEngine, FlutterEngineWeakRef, codec::MessageCodec};

use crate::channel::Channel;
use crate::channel::platform_message::{PlatformMessage, PlatformMessageResponseHandle};
use crate::codec::Value;
use crate::codec::value::{from_value, from_value_owned, to_value};
use crate::error::ValueError;
use log::error;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt;

/// A message Dart sent on a [`MessageChannel`].
///
/// Dart may or may not be waiting for a reply; [`can_respond`](Self::can_respond)
/// says which, and [`respond`](Self::respond) answers.
pub struct Message {
    engine: FlutterEngineWeakRef,
    codec: &'static dyn MessageCodec,
    value: Value,
    response_handle: Option<PlatformMessageResponseHandle>,
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Message")
            .field("value", &self.value)
            .field("can_respond", &self.can_respond())
            .finish_non_exhaustive()
    }
}

impl Message {
    /// Deserialize the message body.
    ///
    /// # Errors
    ///
    /// Fails if the body does not have the shape `T` expects — which is what a
    /// Dart caller passing the wrong thing looks like, so it must be handled
    /// rather than unwrapped.
    pub fn value<'a, T>(&'a self) -> Result<T, ValueError>
    where
        T: Deserialize<'a>,
    {
        from_value(&self.value)
    }

    /// The message body as it was decoded.
    #[must_use]
    pub fn raw_value(&self) -> &Value {
        &self.value
    }

    /// `true` if Dart is waiting for a reply.
    #[must_use]
    pub fn can_respond(&self) -> bool {
        self.response_handle.is_some()
    }

    /// Reply to the message.
    ///
    /// A message Dart did not ask a reply for is logged and dropped: check with
    /// [`can_respond`](Self::can_respond) first to avoid the log line.
    pub fn respond<T>(self, data: T)
    where
        T: Serialize,
    {
        let Some(engine) = self.engine.upgrade() else {
            error!("cannot respond on a channel whose engine is gone");
            return;
        };
        let Some(handle) = self.response_handle else {
            error!("respond() called on a message Dart is not waiting for a reply on");
            return;
        };
        let value = match to_value(data) {
            Ok(value) => value,
            Err(error) => {
                error!("failed to encode a message reply: {error}");
                // The handle still has to be answered or the engine leaks the
                // memory behind it; an empty reply is what the framework reads
                // as "not implemented".
                Value::Null
            }
        };
        let buf = self.codec.encode_message(&value);
        engine.run_on_platform_thread(move |eng| {
            eng.send_platform_message_response(handle, &buf);
        });
    }

    /// A weak handle to the engine this message arrived on.
    #[must_use]
    pub fn engine(&self) -> FlutterEngineWeakRef {
        self.engine.clone()
    }
}

/// Handles the messages Dart sends on one [`MessageChannel`].
pub trait MessageHandler {
    /// Called on the platform thread for every message on the channel.
    fn on_message(&mut self, msg: Message);
}

/// A channel that carries plain values, Dart's `BasicMessageChannel`.
pub struct MessageChannel {
    name: String,
    engine: FlutterEngineWeakRef,
    message_handler: RefCell<Box<dyn MessageHandler>>,
    codec: &'static dyn MessageCodec,
}

impl fmt::Debug for MessageChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MessageChannel")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl MessageChannel {
    /// A channel named `name` that hands its messages to `message_handler`.
    ///
    /// Register it with
    /// [`FlutterEngine::register_channel`](crate::FlutterEngine::register_channel)
    /// to make it live.
    pub fn new<N, H>(name: N, message_handler: H, codec: &'static dyn MessageCodec) -> Self
    where
        N: AsRef<str>,
        H: MessageHandler + 'static,
    {
        Self {
            name: name.as_ref().to_owned(),
            engine: Default::default(),
            message_handler: RefCell::new(Box::new(message_handler)),
            codec,
        }
    }

    fn codec(&self) -> &'static dyn MessageCodec {
        self.codec
    }

    /// Send a value on this channel.
    ///
    /// `value` is encoded on the calling thread and the send itself hops onto
    /// the platform thread, which is the only thread the embedder API accepts
    /// messages from. Already being on it — as a plugin handler is — keeps the
    /// send synchronous, and so keeps it ordered against other sends.
    pub fn send<T>(&self, value: T)
    where
        T: Serialize,
    {
        if let Some(engine) = self.engine() {
            let value = match to_value(value) {
                Ok(value) => value,
                Err(error) => {
                    error!(
                        "failed to encode a message on channel {}: {error}",
                        self.name
                    );
                    return;
                }
            };
            let buf = self.codec().encode_message(&value);
            let name = self.name.clone();
            engine.run_on_platform_thread(move |eng| {
                eng.post_message(&name, &buf);
            });
        }
    }

    /// Send a value on this channel and hand Dart's reply to `callback`.
    ///
    /// `callback` runs on the platform thread. It is not called at all if the
    /// reply cannot be decoded as `V`, which is logged instead.
    pub fn send_with_result<T, F, V>(&self, value: T, callback: F)
    where
        T: Serialize,
        F: FnOnce(V) + 'static + Send,
        V: DeserializeOwned,
    {
        if let Some(engine) = self.engine() {
            let codec = self.codec;
            let value = match to_value(value) {
                Ok(value) => value,
                Err(error) => {
                    error!(
                        "failed to encode a message on channel {}: {error}",
                        self.name
                    );
                    return;
                }
            };
            let buf = codec.encode_message(&value);
            let name = self.name.clone();
            let reply_name = self.name.clone();

            engine.run_on_platform_thread(move |eng| {
                let Some(handle) = PlatformMessageResponseHandle::new(eng, move |data| {
                    let value = match codec.decode_message(data) {
                        Ok(value) => value,
                        Err(error) => {
                            error!("failed to decode a reply on channel {reply_name}: {error}");
                            return;
                        }
                    };
                    match from_value_owned(&value) {
                        Ok(value) => callback(value),
                        Err(error) => {
                            error!(
                                "failed to deserialize a reply on channel {reply_name}: {error}"
                            );
                        }
                    }
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

impl Channel for MessageChannel {
    fn name(&self) -> &str {
        self.name.as_ref()
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
        let value = match codec.decode_message(msg.message) {
            Ok(value) => value,
            Err(error) => {
                error!(
                    "failed to decode a message on channel {}: {error}",
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
        log::trace!("on channel {}, got message {:?}", self.name, value);

        let msg = Message {
            engine: self.engine.clone(),
            value,
            codec,
            response_handle: msg.response_handle,
        };

        self.message_handler.borrow_mut().on_message(msg);
    }
}
