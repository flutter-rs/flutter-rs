use log::error;

use super::{MessageCodec, Value};

pub struct StringCodec;

pub const STRING_CODEC: StringCodec = StringCodec {};

impl MessageCodec for StringCodec {
    fn encode_message(&self, v: &Value) -> Vec<u8> {
        match v {
            Value::String(s) => s.clone().into_bytes(),
            Value::Null => Vec::new(),
            v => {
                error!("Invalid value: {:?}, can only encode string or null", v);
                Vec::new()
            }
        }
    }

    fn decode_message(&self, buf: &[u8]) -> Option<Value> {
        unsafe {
            let s = std::str::from_utf8_unchecked(buf);
            Some(Value::String(s.to_owned()))
        }
    }
}
