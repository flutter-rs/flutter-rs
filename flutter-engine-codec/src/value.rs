use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
};

use serde::{de, ser, Deserialize, Serialize};

pub use self::deserializer::{from_value, Deserializer};

mod deserializer;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Boolean(bool),
    I32(i32),
    I64(i64),
    F64(f64),
    String(String),
    U8List(Vec<u8>),
    I32List(Vec<i32>),
    I64List(Vec<i64>),
    F64List(Vec<f64>),
    List(Vec<Value>),
    Map(HashMap<String, Value>),
}

impl Eq for Value {}

impl Serialize for Value {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Value::Null => serializer.serialize_unit(),
            Value::Boolean(b) => serializer.serialize_bool(*b),
            Value::I32(i) => serializer.serialize_i64(i64::from(*i)),
            Value::I64(i) => serializer.serialize_i64(*i),
            Value::F64(f) => serializer.serialize_f64(*f),
            Value::String(s) => serializer.serialize_str(s.as_str()),
            Value::U8List(vec) => vec.serialize(serializer),
            Value::I32List(vec) => vec.serialize(serializer),
            Value::I64List(vec) => vec.serialize(serializer),
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

impl TryFrom<serde_json::Value> for Value {
    type Error = ();

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
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
                    Err(())
                }
            }
            serde_json::Value::Array(vec) => Ok(Value::List({
                let mut new_vec = Vec::new();
                for v in vec {
                    new_vec.push(v.try_into()?);
                }
                new_vec
            })),
            serde_json::Value::Object(map) => Ok(Value::Map({
                let mut new_map = HashMap::new();
                for (k, v) in map {
                    new_map.insert(k, v.try_into()?);
                }
                new_map
            })),
        }
    }
}
