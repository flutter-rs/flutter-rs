use std::{
    error, fmt,
    sync::mpsc::{RecvError, SendError},
};

use flutter_engine_codec::{MethodCallResult, Value, error::ValueError};

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
pub enum RuntimeMessageError {
    SendError(String),
    RecvError(RecvError),
}

impl fmt::Display for RuntimeMessageError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RuntimeMessageError::SendError(error) => write!(f, "send error: {}", error),
            RuntimeMessageError::RecvError(error) => write!(f, "receive error: {}", error),
        }
    }
}

impl error::Error for RuntimeMessageError {}

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

#[derive(Debug)]
pub enum MessageError {
    ChannelClosed,
    RustError(Box<dyn error::Error>),
    MessageError(RuntimeMessageError),
    CustomError {
        code: String,
        message: String,
        details: Value,
    },
    UnspecifiedError,
}

impl MessageError {
    pub fn from_error<T: error::Error + 'static>(error: T) -> Self {
        MessageError::RustError(Box::new(error))
    }
}

impl fmt::Display for MessageError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MessageError::ChannelClosed => write!(f, "channel already closed"),
            MessageError::RustError(error) => write!(f, "rust error: {}", error),
            MessageError::MessageError(msg) => write!(f, "{}", msg),
            MessageError::CustomError {
                code,
                message,
                details,
            } => write!(f, "{} ({})\ndetails: {:?}", message, code, details),
            MessageError::UnspecifiedError => write!(f, "unspecified error"),
        }
    }
}

impl error::Error for MessageError {
    fn cause(&self) -> Option<&dyn error::Error> {
        match self {
            MessageError::RustError(err) => Some(&**err),
            _ => None,
        }
    }
}

impl From<RuntimeMessageError> for MessageError {
    fn from(error: RuntimeMessageError) -> Self {
        MessageError::MessageError(error)
    }
}

#[derive(Debug)]
pub enum MethodCallError {
    NotImplemented,
    ArgParseError(MethodArgsError),
    DeserializeError(ValueError),
    ChannelClosed,
    MessageError(RuntimeMessageError),
    RustError(Box<dyn error::Error>),
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

impl From<RuntimeMessageError> for MethodCallError {
    fn from(error: RuntimeMessageError) -> Self {
        MethodCallError::MessageError(error)
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
            MethodCallError::MessageError(msg) => write!(f, "{}", msg),
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
    fn cause(&self) -> Option<&dyn error::Error> {
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
            MethodCallError::CustomError {
                code,
                message,
                details,
            } => MethodCallResult::Err {
                code,
                message,
                details,
            },
            error => MethodCallResult::Err {
                code: "".into(),
                message: format!("{}", error),
                details: Value::Null,
            },
        }
    }
}
