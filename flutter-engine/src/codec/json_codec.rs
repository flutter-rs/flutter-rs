use super::{MethodCall, MethodCallResult, MethodCodec, Value};

use log::error;
use serde_json::json;

pub struct JsonMethodCodec;

pub const CODEC: JsonMethodCodec = JsonMethodCodec {};

impl MethodCodec for JsonMethodCodec {
    fn decode_method_call(&self, buf: &[u8]) -> Option<MethodCall> {
        unsafe {
            let s = std::str::from_utf8_unchecked(buf);
            serde_json::from_str::<MethodCall>(s).ok()
        }
    }

    fn decode_envelope(&self, buf: &[u8]) -> Option<MethodCallResult> {
        unsafe {
            let s = std::str::from_utf8_unchecked(buf);
            let json: Value = serde_json::from_str(s).unwrap();
            if let Value::List(mut v) = json {
                if v.len() == 1 {
                    return Some(MethodCallResult::Ok(v.swap_remove(0)));
                } else if v.len() == 3 {
                    return Some(MethodCallResult::Err {
                        code: match &v[0] {
                            Value::String(s) => s.clone(),
                            _ => "".into(),
                        },
                        message: match &v[1] {
                            Value::String(s) => s.clone(),
                            _ => "".into(),
                        },
                        details: v.swap_remove(2),
                    });
                }
            }
            error!("Invalid envelope: {}", s);
            None
        }
    }

    fn encode_method_call(&self, v: &MethodCall) -> Vec<u8> {
        let json = json!(v);
        let s = serde_json::to_string(&json).unwrap();
        s.into_bytes()
    }

    fn encode_success_envelope(&self, v: &Value) -> Vec<u8> {
        let json = json!([v]);
        let s = serde_json::to_string(&json).unwrap();
        s.into_bytes()
    }

    fn encode_error_envelope(&self, code: &str, message: &str, v: &Value) -> Vec<u8> {
        let json = json!([code, message, v]);
        let s = serde_json::to_string(&json).unwrap();
        s.into_bytes()
    }
}
