//! Error types shared by the channel and codec layers.

use std::{
    error,
    sync::mpsc::{RecvError, SendError},
};

use thiserror::Error;

use crate::codec::{MethodCallResult, Value};

/// A boxed error that can cross thread boundaries, as carried by
/// [`MessageError::RustError`] and [`MethodCallError::RustError`].
pub type BoxError = Box<dyn error::Error + Send + Sync>;

/// Failure while pulling typed arguments out of a decoded [`Value`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MethodArgsError {
    /// The field was present but held a value of the wrong type.
    #[error("expected value for type \"{0}\", but found {1:?}")]
    WrongType(String, Value),
    /// The named field was missing from the argument map.
    #[error("missing field \"{0}\" in map")]
    MissingField(String),
}

/// Failure while moving a message between the platform thread and a worker.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RuntimeMessageError {
    /// The receiving end of the channel was dropped.
    #[error("send error: {0}")]
    SendError(String),
    /// The sending end of the channel was dropped.
    #[error("receive error: {0}")]
    RecvError(#[source] RecvError),
}

impl<T> From<SendError<T>> for RuntimeMessageError {
    fn from(error: SendError<T>) -> Self {
        RuntimeMessageError::SendError(format!("{}", error))
    }
}

impl From<RecvError> for RuntimeMessageError {
    fn from(error: RecvError) -> Self {
        RuntimeMessageError::RecvError(error)
    }
}

/// Failure of a [`MessageChannel`](crate::channel::MessageChannel) exchange.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MessageError {
    /// The channel was removed from the registry, or the engine is gone.
    #[error("channel already closed")]
    ChannelClosed,
    /// A handler returned an arbitrary Rust error.
    #[error("rust error: {0}")]
    RustError(#[source] BoxError),
    /// The message could not be handed to the platform thread.
    #[error("{0}")]
    MessageError(#[source] RuntimeMessageError),
    /// A handler reported a Dart-visible error with a code and details.
    #[error("{message} ({code})\ndetails: {details:?}")]
    CustomError {
        /// Dart-visible error code.
        code: String,
        /// Human-readable message.
        message: String,
        /// Structured details handed back to Dart.
        details: Value,
    },
    /// A handler failed without saying why.
    #[error("unspecified error")]
    UnspecifiedError,
}

impl MessageError {
    /// Wrap an arbitrary error as a [`MessageError::RustError`].
    #[must_use]
    pub fn from_error<T: error::Error + Send + Sync + 'static>(error: T) -> Self {
        MessageError::RustError(Box::new(error))
    }
}

impl From<RuntimeMessageError> for MessageError {
    fn from(error: RuntimeMessageError) -> Self {
        MessageError::MessageError(error)
    }
}

/// Failure of a [`MethodChannel`](crate::channel::MethodChannel) call.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MethodCallError {
    /// No handler is registered for the method name.
    #[error("method not implemented")]
    NotImplemented,
    /// The call arguments did not match what the handler expected.
    #[error("failed to parse arguments: {0}")]
    ArgParseError(#[source] MethodArgsError),
    /// The call arguments could not be deserialized into the target type.
    #[error("failed to deserialize value: {0}")]
    DeserializeError(#[source] ValueError),
    /// The channel was removed from the registry, or the engine is gone.
    #[error("channel already closed")]
    ChannelClosed,
    /// The call could not be handed to the platform thread.
    #[error("{0}")]
    MessageError(#[source] RuntimeMessageError),
    /// A handler returned an arbitrary Rust error.
    #[error("rust error: {0}")]
    RustError(#[source] BoxError),
    /// A handler reported a Dart-visible error with a code and details.
    #[error("{message} ({code})\ndetails: {details:?}")]
    CustomError {
        /// Dart-visible error code.
        code: String,
        /// Human-readable message.
        message: String,
        /// Structured details handed back to Dart.
        details: Value,
    },
    /// A handler failed without saying why.
    #[error("unspecified error")]
    UnspecifiedError,
}

impl MethodCallError {
    /// Wrap an arbitrary error as a [`MethodCallError::RustError`].
    #[must_use]
    pub fn from_error<T: error::Error + Send + Sync + 'static>(error: T) -> Self {
        MethodCallError::RustError(Box::new(error))
    }
}

impl From<MethodArgsError> for MethodCallError {
    fn from(error: MethodArgsError) -> Self {
        MethodCallError::ArgParseError(error)
    }
}

impl From<ValueError> for MethodCallError {
    fn from(error: ValueError) -> Self {
        MethodCallError::DeserializeError(error)
    }
}

impl From<RuntimeMessageError> for MethodCallError {
    fn from(error: RuntimeMessageError) -> Self {
        MethodCallError::MessageError(error)
    }
}

impl From<MethodCallError> for MethodCallResult {
    fn from(error: MethodCallError) -> Self {
        match error {
            MethodCallError::NotImplemented => MethodCallResult::NotImplemented,
            MethodCallError::CustomError {
                code,
                message,
                details,
            } => MethodCallResult::Err {
                code,
                message,
                details,
            },
            other => MethodCallResult::Err {
                code: String::new(),
                message: format!("{}", other),
                details: Value::Null,
            },
        }
    }
}

/// Failure while encoding or decoding a [`Value`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ValueError {
    /// A free-form message, as produced by serde.
    #[error("{0}")]
    Message(String),
    /// The value did not have the expected type.
    #[error("wrong type")]
    WrongType,
    /// A list was expected.
    #[error("value is not a list")]
    NoList,
    /// A map was expected.
    #[error("value is not a map")]
    NoMap,
}

impl serde::de::Error for ValueError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        ValueError::Message(msg.to_string())
    }
}
