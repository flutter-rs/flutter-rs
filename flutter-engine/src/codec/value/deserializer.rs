use std::collections::hash_map::Keys;

use serde::{de, de::IntoDeserializer, forward_to_deserialize_any};

use crate::error::ValueError;

use super::Value;

type Result<T> = std::result::Result<T, ValueError>;

pub struct Deserializer<'de> {
    value: &'de Value,
}

impl<'de> Deserializer<'de> {
    pub fn new(value: &'de Value) -> Self {
        Self { value }
    }
}

pub fn from_value<'a, T>(value: &'a Value) -> Result<T>
where
    T: de::Deserialize<'a>,
{
    T::deserialize(&mut Deserializer::new(value))
}

impl<'a, 'de> de::Deserializer<'de> for &'a mut Deserializer<'de> {
    type Error = ValueError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        match self.value {
            Value::Null => visitor.visit_unit(),
            Value::Boolean(b) => visitor.visit_bool(*b),
            Value::I32(i) => visitor.visit_i32(*i),
            Value::I64(i) => visitor.visit_i64(*i),
            Value::F64(f) => visitor.visit_f64(*f),
            Value::String(s) => visitor.visit_str(s.as_str()),
            Value::U8List(_) => visitor.visit_seq(SeqAccess::new(self)),
            Value::I32List(_) => visitor.visit_seq(SeqAccess::new(self)),
            Value::I64List(_) => visitor.visit_seq(SeqAccess::new(self)),
            Value::F64List(_) => visitor.visit_seq(SeqAccess::new(self)),
            Value::List(_) => visitor.visit_seq(SeqAccess::new(self)),
            Value::Map(_) => visitor.visit_map(MapAccess::new(self)),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        match self.value {
            Value::Boolean(b) => visitor.visit_bool(*b),
            _ => Err(ValueError::WrongType),
        }
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        match self.value {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    forward_to_deserialize_any! {
        i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf unit unit_struct
        newtype_struct seq tuple tuple_struct map struct enum identifier ignored_any
    }
}

struct SeqAccess<'a, 'de> {
    de: &'a mut Deserializer<'de>,
    index: usize,
}

impl<'a, 'de> SeqAccess<'a, 'de> {
    pub fn new(de: &'a mut Deserializer<'de>) -> Self {
        Self { de, index: 0 }
    }
}

impl<'a, 'de> de::SeqAccess<'de> for SeqAccess<'a, 'de> {
    type Error = ValueError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: de::DeserializeSeed<'de>,
    {
        match self.de.value {
            Value::U8List(vec) => {
                if vec.len() <= self.index {
                    return Ok(None);
                }
                self.index += 1;
                Ok(Some(
                    seed.deserialize(vec[self.index - 1].into_deserializer())?,
                ))
            }
            Value::I32List(vec) => {
                if vec.len() <= self.index {
                    return Ok(None);
                }
                self.index += 1;
                Ok(Some(
                    seed.deserialize(vec[self.index - 1].into_deserializer())?,
                ))
            }
            Value::I64List(vec) => {
                if vec.len() <= self.index {
                    return Ok(None);
                }
                self.index += 1;
                Ok(Some(
                    seed.deserialize(vec[self.index - 1].into_deserializer())?,
                ))
            }
            Value::F64List(vec) => {
                if vec.len() <= self.index {
                    return Ok(None);
                }
                self.index += 1;
                Ok(Some(
                    seed.deserialize(vec[self.index - 1].into_deserializer())?,
                ))
            }
            Value::List(vec) => {
                if vec.len() <= self.index {
                    return Ok(None);
                }
                self.index += 1;
                Ok(Some(seed.deserialize(&mut Deserializer::new(
                    &vec[self.index - 1],
                ))?))
            }
            _ => Err(ValueError::NoList),
        }
    }
}

struct MapAccess<'a, 'de> {
    de: &'a mut Deserializer<'de>,
    key_iter: Keys<'de, String, Value>,
    next_value: Option<&'de Value>,
}

impl<'a, 'de> MapAccess<'a, 'de> {
    pub fn new(de: &'a mut Deserializer<'de>) -> Self {
        let map = if let Value::Map(map) = de.value {
            map
        } else {
            panic!("deserializer must have a map");
        };
        Self {
            de,
            key_iter: map.keys(),
            next_value: None,
        }
    }
}

impl<'a, 'de> de::MapAccess<'de> for MapAccess<'a, 'de> {
    type Error = ValueError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: de::DeserializeSeed<'de>,
    {
        match self.de.value {
            Value::Map(map) => {
                let next = if let Some(k) = self.key_iter.next() {
                    k
                } else {
                    return Ok(None);
                };
                self.next_value.replace(map.get(next).unwrap());
                Ok(Some(seed.deserialize(next.as_str().into_deserializer())?))
            }
            _ => Err(ValueError::NoMap),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: de::DeserializeSeed<'de>,
    {
        seed.deserialize(&mut Deserializer::new(self.next_value.take().unwrap()))
    }
}
