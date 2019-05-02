use std::{error, fmt};

use crate::codec::{MethodCallResult, Value};

#[derive(Debug)]
pub enum MethodArgsError {
    WrongType(String, Value),
    MissingField(String),
}

impl fmt::Display for MethodArgsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MethodArgsError::MissingField(field) => {
                write!(f, "missing field \"{}\" in map", field.as_str())
            }
            MethodArgsError::WrongType(expected, actual) => write!(
                f,
                "expected value for type \"{}\", but found {:?}",
                expected, actual
            ),
        }
    }
}

impl error::Error for MethodArgsError {}

#[derive(Debug)]
pub enum MethodCallError {
    NotImplemented,
    ArgParseError(MethodArgsError),
    DeserializeError(ValueError),
    ChannelClosed,
    RustError(Box<error::Error>),
    CustomError {
        code: String,
        message: String,
        details: Value,
    },
    UnspecifiedError,
}

impl MethodCallError {
    pub fn from_error<T: error::Error + 'static>(error: T) -> Self {
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

impl fmt::Display for MethodCallError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MethodCallError::NotImplemented => write!(f, "method not implemented"),
            MethodCallError::ArgParseError(err) => write!(f, "failed to parse arguments: {}", err),
            MethodCallError::DeserializeError(err) => {
                write!(f, "failed to deserialize value: {}", err)
            }
            MethodCallError::ChannelClosed => write!(f, "channel already closed"),
            MethodCallError::RustError(error) => write!(f, "rust error: {}", error),
            MethodCallError::CustomError {
                code,
                message,
                details,
            } => write!(f, "{} ({})\ndetails: {:?}", message, code, details),
            MethodCallError::UnspecifiedError => write!(f, "unspecified error"),
        }
    }
}

impl error::Error for MethodCallError {
    fn cause(&self) -> Option<&error::Error> {
        match self {
            MethodCallError::RustError(err) => Some(&**err),
            _ => None,
        }
    }
}

impl Into<MethodCallResult> for MethodCallError {
    fn into(self) -> MethodCallResult {
        match self {
            MethodCallError::NotImplemented => MethodCallResult::NotImplemented,
            MethodCallError::ArgParseError(_) => MethodCallResult::Err {
                code: "".into(),
                message: "failed to parse arguments".into(),
                details: Value::Null,
            },
            MethodCallError::DeserializeError(_) => MethodCallResult::Err {
                code: "".into(),
                message: "failed to deserialize value".into(),
                details: Value::Null,
            },
            MethodCallError::ChannelClosed => MethodCallResult::Err {
                code: "".into(),
                message: "channel closed".into(),
                details: Value::Null,
            },
            MethodCallError::RustError(error) => MethodCallResult::Err {
                code: "".into(),
                message: format!("{}", error),
                details: Value::Null,
            },
            MethodCallError::CustomError {
                code,
                message,
                details,
            } => MethodCallResult::Err {
                code,
                message,
                details,
            },
            MethodCallError::UnspecifiedError => MethodCallResult::Err {
                code: "".into(),
                message: "unspecified error".into(),
                details: Value::Null,
            },
        }
    }
}

#[derive(Debug)]
pub enum ValueError {
    Message(String),
    WrongType,
    NoList,
    NoMap,
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ValueError::Message(s) => write!(f, "{}", s),
            ValueError::WrongType => write!(f, "wrong type"),
            ValueError::NoList => write!(f, "value is not a list"),
            ValueError::NoMap => write!(f, "value is not a map"),
        }
    }
}

impl serde::de::Error for ValueError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        ValueError::Message(msg.to_string())
    }
}

impl error::Error for ValueError {}
