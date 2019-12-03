use std::{error, fmt};

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
