use crate::{codec::MessageCodec, FlutterEngine, FlutterEngineWeakRef};

use crate::channel::platform_message::{PlatformMessage, PlatformMessageResponseHandle};
use crate::channel::Channel;
use crate::codec::value::{from_value, from_value_owned, to_value};
use crate::codec::Value;
use log::error;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;

pub struct Message {
    engine: FlutterEngineWeakRef,
    codec: &'static dyn MessageCodec,
    value: Value,
    response_handle: Option<PlatformMessageResponseHandle>,
}

impl Message {
    pub fn value<'a, T>(&'a self) -> T
    where
        T: Deserialize<'a>,
    {
        from_value(&self.value).unwrap()
    }

    pub fn can_respond(&self) -> bool {
        self.response_handle.is_some()
    }

    pub fn respond<T>(self, data: T)
    where
        T: Serialize,
    {
        if let Some(engine) = self.engine.upgrade() {
            let handle = self
                .response_handle
                .expect("Message can not be response handle");
            let value = to_value(data).expect("Failed to encode data to value");
            let buf = self.codec.encode_message(&value);
            engine.run_on_platform_thread(move |eng| {
                eng.send_platform_message_response(handle, &buf);
            });
        }
    }

    pub fn engine(&self) -> FlutterEngineWeakRef {
        self.engine.clone()
    }
}

pub trait MessageHandler {
    fn on_message(&mut self, msg: Message);
}

pub struct MessageChannel {
    name: String,
    engine: FlutterEngineWeakRef,
    message_handler: RefCell<Box<dyn MessageHandler>>,
    codec: &'static dyn MessageCodec,
}

impl MessageChannel {
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
    pub fn send<T>(&self, value: T)
    where
        T: Serialize,
    {
        if let Some(engine) = self.engine() {
            if !engine.is_platform_thread() {
                panic!("Not on platform thread");
            }

            let buf = self
                .codec()
                .encode_message(&to_value(value).expect("Failed to encode value"));

            engine.send_platform_message(PlatformMessage {
                channel: Cow::Borrowed(self.name()),
                message: &buf,
                response_handle: None,
            });
        }
    }

    pub fn send_with_result<T, F, V>(&self, value: T, callback: F)
    where
        T: Serialize,
        F: FnOnce(V) -> () + 'static + Send,
        V: DeserializeOwned,
    {
        if let Some(engine) = self.engine() {
            if !engine.is_platform_thread() {
                panic!("Not on platform thread");
            }

            let codec = self.codec;
            let buf = codec.encode_message(&to_value(value).unwrap());

            let handle = PlatformMessageResponseHandle::new(engine.clone(), move |data| {
                let resp = codec.decode_message(data);
                let val = resp.unwrap();
                callback(from_value_owned(&val).unwrap());
            });

            engine.send_platform_message(PlatformMessage {
                channel: Cow::Borrowed(self.name()),
                message: &buf,
                response_handle: Some(handle),
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
        let message = codec.decode_message(msg.message).unwrap();
        let channel = self.name().to_owned();
        log::trace!("on channel {}, got message {:?}", channel, message);

        let msg = Message {
            engine: self.engine.clone(),
            value: message,
            codec,
            response_handle: msg.response_handle,
        };

        self.message_handler.borrow_mut().on_message(msg);
    }
}
