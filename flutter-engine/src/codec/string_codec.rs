use super::{MessageCodec, Value};

use log::error;

pub struct StringCodec;

pub const CODEC: StringCodec = StringCodec {};

impl MessageCodec for StringCodec {
    fn encode_message(&self, v: &Value) -> Vec<u8> {
        if let Value::String(s) = v {
            s.clone().into_bytes()
        } else {
            error!("Can only encode string messages");
            Vec::new()
        }
    }

    fn decode_message(&self, buf: &[u8]) -> Option<Value> {
        unsafe {
            let s = std::str::from_utf8_unchecked(buf);
            Some(Value::String(s.to_owned()))
        }
    }
}
