use serde::{Deserialize, Serialize};

pub use self::value::Value;

pub mod json_codec;
pub mod standard_codec;
#[macro_use]
pub mod value;

#[derive(Serialize, Deserialize, Debug)]
pub struct MethodCall {
    pub method: String,
    pub args: Value,
}

pub enum MethodCallResult {
    Ok(Value),
    Err {
        code: String,
        message: String,
        details: Value,
    },
    NotImplemented,
}

pub trait MethodCodec {
    /// Methods for handling dart call
    fn decode_method_call(&self, buf: &[u8]) -> Option<MethodCall>;
    fn encode_success_envelope(&self, v: &Value) -> Vec<u8>;
    fn encode_error_envelope(&self, code: &str, message: &str, details: &Value) -> Vec<u8>;

    /// Methods for calling into dart
    fn encode_method_call(&self, v: &MethodCall) -> Vec<u8>;
    fn decode_envelope(&self, buf: &[u8]) -> Option<MethodCallResult>;
}
