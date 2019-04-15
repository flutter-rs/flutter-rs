use super::{MethodCall, MethodCallResult, MethodCodec};

use log::error;
use serde_json::json;
pub use serde_json::Value;

pub struct JsonMethodCodec;

impl MethodCodec for JsonMethodCodec {
    type R = Value;

    fn decode_method_call(buf: &[u8]) -> Option<MethodCall<Self::R>> {
        unsafe {
            let s = std::str::from_utf8_unchecked(buf);
            serde_json::from_str::<MethodCall<Self::R>>(s).ok()
        }
    }

    fn decode_envelope(buf: &[u8]) -> Option<MethodCallResult<Self::R>> {
        unsafe {
            let s = std::str::from_utf8_unchecked(buf);
            let json: Value = serde_json::from_str(s).unwrap();
            if let Value::Array(mut v) = json {
                if v.len() == 1 {
                    return Some(MethodCallResult::Ok(v.swap_remove(0)));
                } else if v.len() == 3 {
                    return Some(MethodCallResult::Err {
                        code: v[0].as_str().unwrap().to_owned(),
                        message: v[1].as_str().unwrap().to_owned(),
                        details: v.swap_remove(2),
                    });
                }
            }
            error!("Invalid envelope: {}", s);
            None
        }
    }

    fn encode_method_call(v: &MethodCall<Self::R>) -> Vec<u8> {
        let json = json!(v);
        let s = serde_json::to_string(&json).unwrap();
        s.into_bytes()
    }

    fn encode_success_envelope(v: &Self::R) -> Vec<u8> {
        let json = json!([v]);
        let s = serde_json::to_string(&json).unwrap();
        s.into_bytes()
    }

    fn encode_error_envelope(code: &str, message: &str, v: &Self::R) -> Vec<u8> {
        let json = json!([code, message, v]);
        let s = serde_json::to_string(&json).unwrap();
        s.into_bytes()
    }
}
