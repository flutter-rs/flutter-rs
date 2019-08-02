use super::{MethodCall, MethodCallResult, MethodCodec, Value};

use log::error;

pub struct StringCodec;

pub const CODEC: StringCodec = StringCodec {};

impl MethodCodec for StringCodec {
    fn decode_method_call(&self, _: &[u8]) -> Option<MethodCall> {
        error!("Can't decode method calls");
        None
    }

    fn encode_success_envelope(&self, _: &Value) -> Vec<u8> {
        error!("Can't encode success envelopes");
        Vec::new()
    }

    fn encode_error_envelope(&self, _: &str, _: &str, _: &Value) -> Vec<u8> {
        error!("Can't encode error envelopes");
        Vec::new()
    }

    fn encode_method_call(&self, _: &MethodCall) -> Vec<u8> {
        error!("Can't encode method calls");
        Vec::new()
    }

    fn decode_envelope(&self, _: &[u8]) -> Option<MethodCallResult> {
        error!("Can't decode envelopes");
        None
    }

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
