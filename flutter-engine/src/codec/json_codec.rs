//! Dart's `JSONMethodCodec`: UTF-8 JSON text.
//!
//! A method call is `{"method": .., "args": ..}`; a reply envelope is a
//! one-element array for success and a three-element `[code, message, details]`
//! array for an error.

use serde::Serialize;

use super::{DecodeError, MessageCodec, MethodCall, MethodCallResult, MethodCodec, Value};

/// Dart's `JSONMethodCodec` / `JSONMessageCodec`.
#[derive(Debug)]
pub struct JsonMethodCodec;

/// The shared [`JsonMethodCodec`] instance to hand to a channel.
pub const JSON_CODEC: JsonMethodCodec = JsonMethodCodec {};

/// Decode engine-supplied bytes as JSON text.
///
/// The bytes come from Dart, which writes UTF-8, but they are still untrusted
/// input: validating here is what keeps a non-UTF-8 payload from becoming an
/// invalid `&str`.
fn from_json<T: serde::de::DeserializeOwned>(buf: &[u8]) -> Result<T, DecodeError> {
    let s = std::str::from_utf8(buf).map_err(|_| DecodeError::NotUtf8)?;
    serde_json::from_str(s).map_err(DecodeError::Json)
}

impl MethodCodec for JsonMethodCodec {
    fn decode_method_call(&self, buf: &[u8]) -> Result<MethodCall, DecodeError> {
        from_json(buf)
    }

    fn decode_envelope(&self, buf: &[u8]) -> Result<MethodCallResult, DecodeError> {
        // An empty envelope is how the framework says "no plugin handled this".
        if buf.is_empty() {
            return Ok(MethodCallResult::NotImplemented);
        }

        let mut list = match from_json(buf)? {
            Value::List(list) => list,
            _ => {
                return Err(DecodeError::WrongType {
                    expected: "an envelope array",
                });
            }
        };
        match list.len() {
            1 => Ok(MethodCallResult::Ok(list.swap_remove(0))),
            3 => Ok(MethodCallResult::Err {
                code: match &list[0] {
                    Value::String(s) => s.clone(),
                    _ => String::new(),
                },
                message: match &list[1] {
                    Value::String(s) => s.clone(),
                    _ => String::new(),
                },
                details: list.swap_remove(2),
            }),
            _ => Err(DecodeError::WrongType {
                expected: "an envelope array of one (success) or three (error) elements",
            }),
        }
    }

    fn encode_method_call(&self, v: &MethodCall) -> Vec<u8> {
        to_json(v)
    }

    fn encode_success_envelope(&self, v: &Value) -> Vec<u8> {
        to_json(&[v])
    }

    fn encode_error_envelope(&self, code: &str, message: &str, v: &Value) -> Vec<u8> {
        // A tuple serializes as a JSON array, which is what the envelope is.
        to_json(&(code, message, v))
    }
}

impl MessageCodec for JsonMethodCodec {
    fn encode_message(&self, v: &Value) -> Vec<u8> {
        to_json(v)
    }

    fn decode_message(&self, buf: &[u8]) -> Result<Value, DecodeError> {
        from_json(buf)
    }
}

/// Serialize straight to JSON text, without building an intermediate
/// `serde_json::Value` tree first.
///
/// Serialization only fails if the `Serialize` impl itself errors, which none of
/// the types passed here can do: a [`Value`] is JSON-shaped by construction and
/// its maps are keyed by `String`.
fn to_json<T: Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec(value).expect("BUG: a Value is always representable as JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_calls_round_trip() {
        let encoded = JSON_CODEC.encode_method_call(&MethodCall {
            method: "foo".into(),
            args: Value::I64(7),
        });
        assert_eq!(
            String::from_utf8_lossy(&encoded),
            r#"{"method":"foo","args":7}"#
        );

        let decoded = JSON_CODEC
            .decode_method_call(&encoded)
            .expect("a method call round-trips");
        assert_eq!(decoded.method, "foo");
        assert_eq!(decoded.args, Value::I64(7));
    }

    /// A non-UTF-8 payload used to be handed to `from_utf8_unchecked`, which is
    /// instant undefined behaviour.
    #[test]
    fn invalid_utf8_is_rejected() {
        assert!(matches!(
            JSON_CODEC.decode_message(&[0xff, 0xfe]),
            Err(DecodeError::NotUtf8)
        ));
        assert!(matches!(
            JSON_CODEC.decode_method_call(&[0xff, 0xfe]),
            Err(DecodeError::NotUtf8)
        ));
        assert!(matches!(
            JSON_CODEC.decode_envelope(&[0xff, 0xfe]),
            Err(DecodeError::NotUtf8)
        ));
    }

    /// Malformed JSON used to be `.unwrap()`ed inside an `extern "C"` callback.
    #[test]
    fn malformed_json_is_rejected() {
        assert!(matches!(
            JSON_CODEC.decode_envelope(b"[1,"),
            Err(DecodeError::Json(_))
        ));
        assert!(matches!(
            JSON_CODEC.decode_message(b"{"),
            Err(DecodeError::Json(_))
        ));
    }

    #[test]
    fn envelopes_round_trip() {
        let ok = JSON_CODEC.encode_success_envelope(&Value::I64(42));
        assert_eq!(String::from_utf8_lossy(&ok), "[42]");
        assert!(matches!(
            JSON_CODEC.decode_envelope(&ok).expect("a success envelope"),
            MethodCallResult::Ok(Value::I64(42))
        ));

        let err = JSON_CODEC.encode_error_envelope("E", "m", &Value::Null);
        assert_eq!(String::from_utf8_lossy(&err), r#"["E","m",null]"#);
        match JSON_CODEC.decode_envelope(&err).expect("an error envelope") {
            MethodCallResult::Err { code, message, .. } => {
                assert_eq!(code, "E");
                assert_eq!(message, "m");
            }
            other => panic!("expected an error envelope, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_envelope_is_not_implemented() {
        assert!(matches!(
            JSON_CODEC.decode_envelope(&[]).expect("an empty envelope"),
            MethodCallResult::NotImplemented
        ));
    }

    #[test]
    fn an_envelope_of_the_wrong_shape_is_rejected() {
        assert!(matches!(
            JSON_CODEC.decode_envelope(b"[1,2]"),
            Err(DecodeError::WrongType { .. })
        ));
        assert!(matches!(
            JSON_CODEC.decode_envelope(b"7"),
            Err(DecodeError::WrongType { .. })
        ));
    }
}
