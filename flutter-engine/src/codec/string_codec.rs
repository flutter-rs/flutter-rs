//! Dart's `StringCodec`: the message body *is* the UTF-8 text.

use log::error;

use super::{DecodeError, MessageCodec, Value};

/// Dart's `StringCodec`.
#[derive(Debug)]
pub struct StringCodec;

/// The shared [`StringCodec`] instance to hand to a channel.
pub const STRING_CODEC: StringCodec = StringCodec {};

impl MessageCodec for StringCodec {
    fn encode_message(&self, v: &Value) -> Vec<u8> {
        match v {
            Value::String(s) => s.clone().into_bytes(),
            Value::Null => Vec::new(),
            v => {
                error!("Invalid value: {v:?}, can only encode string or null");
                Vec::new()
            }
        }
    }

    fn decode_message(&self, buf: &[u8]) -> Result<Value, DecodeError> {
        // Dart writes UTF-8, so the replacement characters only appear for a
        // corrupted or mis-codec'd message. Handing the bytes to
        // `from_utf8_unchecked`, as this used to, made an invalid `str` that
        // plugin code then read.
        Ok(Value::String(String::from_utf8_lossy(buf).into_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_round_trip() {
        let encoded = STRING_CODEC.encode_message(&Value::String("héllo".into()));
        assert_eq!(encoded, "héllo".as_bytes());
        assert_eq!(
            STRING_CODEC
                .decode_message(&encoded)
                .expect("a string decodes"),
            Value::String("héllo".into())
        );
    }

    #[test]
    fn null_encodes_as_an_empty_message() {
        assert!(STRING_CODEC.encode_message(&Value::Null).is_empty());
    }

    /// This used to produce an invalid `str` through `from_utf8_unchecked`.
    #[test]
    fn invalid_utf8_is_replaced_not_undefined() {
        assert_eq!(
            STRING_CODEC
                .decode_message(&[0xff, b'a'])
                .expect("invalid UTF-8 decodes lossily"),
            Value::String("\u{fffd}a".into())
        );
    }
}
