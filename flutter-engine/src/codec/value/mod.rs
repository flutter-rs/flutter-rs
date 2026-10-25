//! [`Value`], the shape every codec decodes to, and the serde glue around it.

use crate::error::MethodCallError;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};

use serde::{Deserialize, Serialize, de, ser};
use thiserror::Error;

pub use self::deserializer::{Deserializer, from_value, from_value_owned};

mod deserializer;
mod serializer;

/// Push serializable values onto a `Vec<Value>`, the shape a positional
/// argument list takes on a method channel.
pub trait VecExt {
    /// Serialize `value` and push it.
    ///
    /// # Errors
    ///
    /// Fails if `value` has no [`Value`] representation — see [`to_value`].
    fn push_as_value<T>(&mut self, value: T) -> Result<(), Error>
    where
        T: Serialize;
}

impl VecExt for Vec<Value> {
    fn push_as_value<T>(&mut self, value: T) -> Result<(), Error>
    where
        T: Serialize,
    {
        self.push(to_value(value)?);
        Ok(())
    }
}

/// A value on a platform channel.
///
/// The variants line up one for one with the type tags of Dart's
/// `StandardMessageCodec`, which is also why this enum is
/// `#[non_exhaustive]`: the wire format grows (`F32List` was added for Dart's
/// `Float32List`), and a downstream `match` should not break when it does.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// Dart's `null`.
    Null,
    /// A `bool`.
    Boolean(bool),
    /// An `int` that fits in 32 bits.
    I32(i32),
    /// An `int` that needs 64 bits.
    I64(i64),
    /// A `double`.
    F64(f64),
    /// A `String`.
    String(String),
    /// A `Uint8List`.
    U8List(Vec<u8>),
    /// An `Int32List`.
    I32List(Vec<i32>),
    /// An `Int64List`.
    I64List(Vec<i64>),
    /// A `Float32List`.
    F32List(Vec<f32>),
    /// A `Float64List`.
    F64List(Vec<f64>),
    /// A `List` of arbitrary values.
    List(Vec<Value>),
    /// A `Map` with string keys.
    Map(HashMap<String, Value>),
}

impl Serialize for Value {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Value::Null => serializer.serialize_unit(),
            Value::Boolean(b) => serializer.serialize_bool(*b),
            // `serialize_i32`, not `serialize_i64`: it is what lets
            // `to_value` give an `I32` back as an `I32`. JSON renders both the
            // same way, so the wire format of `JSON_CODEC` is unchanged.
            Value::I32(i) => serializer.serialize_i32(*i),
            Value::I64(i) => serializer.serialize_i64(*i),
            Value::F64(f) => serializer.serialize_f64(*f),
            Value::String(s) => serializer.serialize_str(s.as_str()),
            Value::U8List(vec) => vec.serialize(serializer),
            Value::I32List(vec) => vec.serialize(serializer),
            Value::I64List(vec) => vec.serialize(serializer),
            Value::F32List(vec) => vec.serialize(serializer),
            Value::F64List(vec) => vec.serialize(serializer),
            Value::List(vec) => vec.serialize(serializer),
            Value::Map(m) => {
                use ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(m.len()))?;
                for (k, v) in m {
                    map.serialize_key(k)?;
                    map.serialize_value(v)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        use de::Visitor;
        struct ValueVisitor;

        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = Value;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("any valid JSON value")
            }

            #[inline]
            fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
                Ok(Value::Boolean(value))
            }

            #[inline]
            fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
                Ok(Value::I64(value))
            }

            #[inline]
            fn visit_u64<E>(self, value: u64) -> Result<Value, E>
            where
                E: de::Error,
            {
                if let Ok(i) = i64::try_from(value) {
                    Ok(Value::I64(i))
                } else {
                    Err(E::custom("number too large for i64"))
                }
            }

            #[inline]
            fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
                Ok(Value::F64(value))
            }

            #[inline]
            fn visit_str<E>(self, value: &str) -> Result<Value, E> {
                Ok(Value::String(value.into()))
            }

            #[inline]
            fn visit_string<E>(self, value: String) -> Result<Value, E> {
                Ok(Value::String(value))
            }

            #[inline]
            fn visit_none<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }

            #[inline]
            fn visit_some<D>(self, deserializer: D) -> Result<Value, D::Error>
            where
                D: de::Deserializer<'de>,
            {
                Deserialize::deserialize(deserializer)
            }

            #[inline]
            fn visit_unit<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }

            #[inline]
            fn visit_seq<V>(self, mut visitor: V) -> Result<Value, V::Error>
            where
                V: de::SeqAccess<'de>,
            {
                // Deliberately not pre-sized from `size_hint`: for a
                // self-describing format that hint comes from the input.
                let mut vec = Vec::new();
                while let Some(elem) = visitor.next_element()? {
                    vec.push(elem);
                }
                Ok(Value::List(vec))
            }

            #[inline]
            fn visit_map<V>(self, mut visitor: V) -> Result<Value, V::Error>
            where
                V: de::MapAccess<'de>,
            {
                // Not pre-sized, for the same reason as `visit_seq` above.
                let mut map = HashMap::new();
                while let Some((k, v)) = visitor.next_entry()? {
                    map.insert(k, v);
                }
                Ok(Value::Map(map))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

/// Failure while turning a Rust value into a [`Value`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// A number outside the range a [`Value`] can hold: Dart's `int` is a
    /// signed 64-bit integer, so a `u64` above `i64::MAX` has no
    /// representation.
    #[error("number is out of range")]
    NumberOutOfRange,
    /// A map key with no text form. [`Value::Map`] is keyed by `String`, so a
    /// key has to be a string, a `char`, an integer, a `bool` or a unit enum
    /// variant.
    #[error("map key cannot be represented as a string")]
    NonStringKey,
    /// A [`Serialize`] implementation reported a failure of its own, or
    /// produced something with no [`Value`] representation.
    #[error("{0}")]
    Message(String),
}

impl ser::Error for Error {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

impl From<Error> for MethodCallError {
    fn from(error: Error) -> Self {
        Self::from_error(error)
    }
}

impl TryFrom<serde_json::Value> for Value {
    type Error = Error;

    fn try_from(value: serde_json::Value) -> Result<Self, Error> {
        match value {
            serde_json::Value::Null => Ok(Value::Null),
            serde_json::Value::Bool(b) => Ok(Value::Boolean(b)),
            serde_json::Value::String(s) => Ok(Value::String(s)),
            serde_json::Value::Number(num) => {
                if let Some(i) = num.as_i64() {
                    Ok(Value::I64(i))
                } else if let Some(f) = num.as_f64() {
                    Ok(Value::F64(f))
                } else {
                    Err(Error::NumberOutOfRange)
                }
            }
            serde_json::Value::Array(vec) => Ok(Value::List({
                let mut new_vec = Vec::with_capacity(vec.len());
                for v in vec {
                    new_vec.push(v.try_into()?);
                }
                new_vec
            })),
            serde_json::Value::Object(map) => Ok(Value::Map({
                let mut new_map = HashMap::with_capacity(map.len());
                for (k, v) in map {
                    new_map.insert(k, v.try_into()?);
                }
                new_map
            })),
        }
    }
}

/// Serialize `value` into a [`Value`].
///
/// A `Vec<u8>` becomes a [`Value::U8List`] and a `Vec<f32>` a
/// [`Value::F32List`], so Dart receives a `Uint8List`/`Float32List` rather
/// than a `List` of boxed numbers. An *empty* sequence cannot be narrowed that
/// way — serde reports the element type only through the elements — so
/// `Vec::<u8>::new()` becomes an empty [`Value::List`]; build the [`Value`]
/// yourself if the tag matters for an empty buffer. Sequences of `i32`, `i64`
/// and `f64` deliberately stay a [`Value::List`]; see the `serializer` module
/// documentation for why.
///
/// Non-finite floats are passed through as [`Value::F64`]: the standard codec
/// writes raw IEEE-754 bits and Dart's `double` has NaN and both infinities.
/// [`JSON_CODEC`](crate::codec::JSON_CODEC) still writes them as `null`,
/// because JSON has no spelling for them.
///
/// # Errors
///
/// Fails for anything with no [`Value`] representation — a map keyed by a
/// tuple, or a `u64` above `i64::MAX`. See [`enum@Error`].
pub fn to_value<T: Serialize>(value: T) -> Result<Value, Error> {
    serializer::to_value(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_as_value_reports_an_unrepresentable_value() {
        let mut args: Vec<Value> = Vec::new();
        args.push_as_value(7_i32).expect("an i32 is representable");
        assert_eq!(args, vec![Value::I32(7)]);

        // A map keyed by a tuple has no `Value` representation; this used to
        // panic inside plugin code.
        let unrepresentable = HashMap::from([((1_i32, 2_i32), "pair")]);
        assert!(args.push_as_value(unrepresentable).is_err());
        assert_eq!(args.len(), 1, "a failed push must not add anything");
    }

    /// The old `to_value` went through `serde_json`, which rejected a
    /// non-string key with a message about JSON; the error now names the real
    /// problem.
    #[test]
    fn a_non_string_map_key_is_reported_as_such() {
        let error =
            to_value(HashMap::from([((1_i32, 2_i32), "pair")])).expect_err("not representable");
        assert!(
            matches!(error, Error::NonStringKey),
            "expected NonStringKey, got {error:?}"
        );
        assert_eq!(
            error.to_string(),
            "map key cannot be represented as a string"
        );
    }

    /// A `u64` above `i64::MAX` is the one number Dart cannot hold.
    #[test]
    fn a_number_out_of_range_is_reported() {
        assert!(matches!(
            to_value(u64::MAX).expect_err("u64::MAX does not fit a Dart int"),
            Error::NumberOutOfRange
        ));
        assert_eq!(
            to_value(i64::MAX as u64).expect("i64::MAX fits"),
            Value::I64(i64::MAX)
        );
    }

    /// Serializing a [`Value`] has to give the same [`Value`] back, or a
    /// plugin that hands one straight to `invoke_method` silently changes what
    /// Dart receives. Only the three list variants serde cannot tell apart
    /// from a `Value::List` are excluded; see the `serializer` module docs.
    #[test]
    fn serializing_a_value_gives_the_same_value_back() {
        for value in [
            Value::Null,
            Value::Boolean(true),
            Value::I32(-7),
            Value::I64(1 << 40),
            Value::F64(1.5),
            Value::String("text".into()),
            Value::U8List(vec![1, 2, 3]),
            Value::F32List(vec![1.5, 2.5]),
            Value::List(vec![Value::I32(1), Value::String("a".into()), Value::Null]),
            Value::Map(HashMap::from([("k".to_owned(), Value::I32(1))])),
        ] {
            assert_eq!(
                to_value(&value).expect("a Value is representable"),
                value,
                "round-tripping {value:?}"
            );
        }
    }

    /// `I32List`, `I64List` and `F64List` are indistinguishable from a
    /// `Value::List` of the matching scalar once serde has seen them, so they
    /// widen rather than risk turning a hand-built list into a typed one.
    #[test]
    fn the_ambiguous_typed_lists_widen_to_a_plain_list() {
        assert_eq!(
            to_value(Value::I32List(vec![1, 2])).expect("representable"),
            Value::List(vec![Value::I32(1), Value::I32(2)])
        );
        assert_eq!(
            to_value(Value::I64List(vec![1])).expect("representable"),
            Value::List(vec![Value::I64(1)])
        );
        assert_eq!(
            to_value(Value::F64List(vec![1.5])).expect("representable"),
            Value::List(vec![Value::F64(1.5)])
        );
    }
}
