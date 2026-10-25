//! The wire formats a channel can speak.
//!
//! A codec turns the bytes the engine carries on a platform channel into a
//! [`Value`] and back. Three are built in and mirror the Dart side exactly:
//! [`STANDARD_CODEC`] (Dart's `StandardMethodCodec`), [`JSON_CODEC`]
//! (`JSONMethodCodec`) and [`STRING_CODEC`] (`StringCodec`).
//!
//! Decoding is parsing untrusted input: the bytes come from the Dart side of
//! the same app, but a mismatched codec, a truncated message or a type tag this
//! crate does not know must all come back as a [`DecodeError`] rather than a
//! panic — the decode happens inside an `extern "C"` engine callback, where a
//! panic cannot unwind.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use self::value::Value;

mod json_codec;
mod standard_codec;
mod string_codec;
#[macro_use]
pub mod value;

pub use json_codec::{JSON_CODEC, JsonMethodCodec};
pub use standard_codec::{STANDARD_CODEC, StandardMethodCodec};
pub use string_codec::{STRING_CODEC, StringCodec};

/// Why a codec could not decode a message.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DecodeError {
    /// The buffer ended in the middle of a value.
    #[error("the message ended in the middle of a value")]
    Truncated,
    /// A type tag this codec does not know, which usually means the two sides
    /// disagree about the channel's codec.
    #[error("unknown type tag {0}")]
    UnknownType(u8),
    /// A well-formed value, but not the one the position called for.
    #[error("expected {expected}")]
    WrongType {
        /// What the codec was looking for.
        expected: &'static str,
    },
    /// Bytes that are not valid UTF-8 where text was expected.
    #[error("the message is not valid UTF-8")]
    NotUtf8,
    /// Malformed JSON, on a channel using [`JSON_CODEC`].
    #[error("malformed JSON: {0}")]
    Json(#[source] serde_json::Error),
}

/// A method call as it arrives from, or goes out to, Dart.
///
/// The field names are part of the JSON wire format
/// (`{"method": .., "args": ..}`), so they must keep their spelling.
#[derive(Serialize, Deserialize, Debug)]
pub struct MethodCall {
    /// The method name.
    pub method: String,
    /// The single argument value; `Value::Null` when Dart passed none.
    pub args: Value,
}

/// The outcome of a method call, in the shape the envelope encodes.
///
/// Deliberately *not* `#[non_exhaustive]`: these are the three outcomes the
/// Dart protocol defines, and every caller has to handle all three.
#[derive(Debug)]
pub enum MethodCallResult {
    /// The call succeeded with this result value.
    Ok(Value),
    /// The call failed with a Dart-visible error.
    Err {
        /// Dart-visible error code.
        code: String,
        /// Human-readable message.
        message: String,
        /// Structured details handed back to Dart.
        details: Value,
    },
    /// Nothing handled the method; encoded as an empty envelope, which Dart
    /// turns into a `MissingPluginException`.
    NotImplemented,
}

/// A codec for a channel that carries method calls and their replies.
pub trait MethodCodec: Send + Sync {
    /// Decode a method call Dart sent us.
    ///
    /// # Errors
    ///
    /// Fails if `buf` is not a method call in this codec's format.
    fn decode_method_call(&self, buf: &[u8]) -> Result<MethodCall, DecodeError>;

    /// Encode a successful reply.
    ///
    /// # Panics
    ///
    /// Panics if a string or list in `v` is longer than `u32::MAX` bytes, which
    /// the wire format cannot express.
    fn encode_success_envelope(&self, v: &Value) -> Vec<u8>;

    /// Encode an error reply.
    ///
    /// # Panics
    ///
    /// As for [`encode_success_envelope`](Self::encode_success_envelope).
    fn encode_error_envelope(&self, code: &str, message: &str, details: &Value) -> Vec<u8>;

    /// Encode whichever reply `response` calls for.
    ///
    /// # Panics
    ///
    /// As for [`encode_success_envelope`](Self::encode_success_envelope).
    fn encode_method_call_response(&self, response: &MethodCallResult) -> Vec<u8> {
        match response {
            MethodCallResult::Ok(data) => self.encode_success_envelope(data),
            MethodCallResult::Err {
                code,
                message,
                details,
            } => self.encode_error_envelope(code, message, details),
            MethodCallResult::NotImplemented => vec![],
        }
    }

    /// Encode a method call for Dart.
    ///
    /// # Panics
    ///
    /// As for [`encode_success_envelope`](Self::encode_success_envelope).
    fn encode_method_call(&self, v: &MethodCall) -> Vec<u8>;

    /// Decode the reply envelope Dart sent back for a call of ours.
    ///
    /// # Errors
    ///
    /// Fails if `buf` is not a reply envelope in this codec's format.
    fn decode_envelope(&self, buf: &[u8]) -> Result<MethodCallResult, DecodeError>;
}

/// A codec for a channel that carries plain values in both directions.
pub trait MessageCodec: Send + Sync {
    /// Encode a value for Dart.
    ///
    /// # Panics
    ///
    /// Panics if a string or list in `v` is longer than `u32::MAX` bytes, which
    /// the wire format cannot express.
    fn encode_message(&self, v: &Value) -> Vec<u8>;

    /// Decode a value Dart sent us.
    ///
    /// # Errors
    ///
    /// Fails if `buf` is not a value in this codec's format.
    fn decode_message(&self, buf: &[u8]) -> Result<Value, DecodeError>;
}
