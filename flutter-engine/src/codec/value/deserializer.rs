//! A serde [`Deserializer`] that reads straight out of a [`Value`] tree.

use std::collections::HashMap;
use std::collections::hash_map;

use serde::{de, de::IntoDeserializer, forward_to_deserialize_any};

use crate::error::ValueError;

use super::Value;

type Result<T> = std::result::Result<T, ValueError>;

/// Deserialize a Rust type out of a borrowed [`Value`].
///
/// Strings and byte buffers are handed to the visitor as borrows of the
/// `Value`, so a target type can borrow from it instead of allocating.
#[derive(Debug)]
pub struct Deserializer<'de> {
    value: &'de Value,
}

impl<'de> Deserializer<'de> {
    /// A deserializer reading from `value`.
    #[must_use]
    pub fn new(value: &'de Value) -> Self {
        Self { value }
    }
}

/// Deserialize `T` out of `value`, borrowing from it where `T` allows.
///
/// # Errors
///
/// Fails if `value` does not have the shape `T` expects.
pub fn from_value<'a, T>(value: &'a Value) -> Result<T>
where
    T: de::Deserialize<'a>,
{
    T::deserialize(&mut Deserializer::new(value))
}

/// Deserialize an owned `T` out of `value`.
///
/// # Errors
///
/// Fails if `value` does not have the shape `T` expects.
pub fn from_value_owned<T>(value: &Value) -> Result<T>
where
    T: de::DeserializeOwned,
{
    T::deserialize(&mut Deserializer::new(value))
}

impl<'de> de::Deserializer<'de> for &mut Deserializer<'de> {
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
            // Borrowed, not copied: `self.value` outlives the visitor by the
            // struct definition, which is what lets `from_value`'s `'a` mean
            // something.
            Value::String(s) => visitor.visit_borrowed_str(s.as_str()),
            Value::U8List(_)
            | Value::I32List(_)
            | Value::I64List(_)
            | Value::F32List(_)
            | Value::F64List(_)
            | Value::List(_) => visitor.visit_seq(SeqAccess::new(self)),
            Value::Map(map) => visitor.visit_map(MapAccess::new(map)),
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

    /// A `Uint8List` is handed over as one borrowed byte slice rather than
    /// element by element through `deserialize_any`.
    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        match self.value {
            Value::U8List(bytes) => visitor.visit_borrowed_bytes(bytes),
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
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

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        match self.value {
            Value::String(s) => visitor.visit_enum(s.clone().into_deserializer()),
            // A non-unit variant is a single-entry map of variant name to body.
            Value::Map(m) if m.len() == 1 => {
                let (name, body) = m.iter().next().expect("the map holds one entry");
                visitor.visit_enum(EnumAccess::new(name, body))
            }
            _ => Err(ValueError::WrongType),
        }
    }

    forward_to_deserialize_any! {
        i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string unit unit_struct
        newtype_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}

/// Deserialize `vec[*index]` as a scalar and advance `index`.
fn next_scalar<'de, T, E>(seed: T, vec: &[E], index: &mut usize) -> Result<Option<T::Value>>
where
    T: de::DeserializeSeed<'de>,
    E: Copy + IntoDeserializer<'de, ValueError>,
{
    let Some(&element) = vec.get(*index) else {
        return Ok(None);
    };
    *index += 1;
    Ok(Some(seed.deserialize(element.into_deserializer())?))
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

impl<'de> de::SeqAccess<'de> for SeqAccess<'_, 'de> {
    type Error = ValueError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: de::DeserializeSeed<'de>,
    {
        match self.de.value {
            Value::U8List(vec) => next_scalar(seed, vec, &mut self.index),
            Value::I32List(vec) => next_scalar(seed, vec, &mut self.index),
            Value::I64List(vec) => next_scalar(seed, vec, &mut self.index),
            Value::F32List(vec) => next_scalar(seed, vec, &mut self.index),
            Value::F64List(vec) => next_scalar(seed, vec, &mut self.index),
            Value::List(vec) => {
                let Some(element) = vec.get(self.index) else {
                    return Ok(None);
                };
                self.index += 1;
                Ok(Some(seed.deserialize(&mut Deserializer::new(element))?))
            }
            _ => Err(ValueError::NoList),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        match self.de.value {
            Value::U8List(vec) => Some(vec.len()),
            Value::I32List(vec) => Some(vec.len()),
            Value::I64List(vec) => Some(vec.len()),
            Value::F32List(vec) => Some(vec.len()),
            Value::F64List(vec) => Some(vec.len()),
            Value::List(vec) => Some(vec.len()),
            _ => None,
        }
        .map(|len| len.saturating_sub(self.index))
    }
}

struct MapAccess<'de> {
    iter: hash_map::Iter<'de, String, Value>,
    next_value: Option<&'de Value>,
}

impl<'de> MapAccess<'de> {
    pub fn new(map: &'de HashMap<String, Value>) -> Self {
        Self {
            iter: map.iter(),
            next_value: None,
        }
    }
}

impl<'de> de::MapAccess<'de> for MapAccess<'de> {
    type Error = ValueError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.iter.next() else {
            return Ok(None);
        };
        self.next_value = Some(value);
        Ok(Some(seed.deserialize(key.as_str().into_deserializer())?))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: de::DeserializeSeed<'de>,
    {
        let Some(value) = self.next_value.take() else {
            return Err(de::Error::custom(
                "next_value_seed called before next_key_seed",
            ));
        };
        seed.deserialize(&mut Deserializer::new(value))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

struct EnumAccess<'de> {
    name: &'de String,
    value_deserializer: Deserializer<'de>,
}

impl<'de> EnumAccess<'de> {
    pub fn new(name: &'de String, value: &'de Value) -> Self {
        Self {
            name,
            value_deserializer: Deserializer::new(value),
        }
    }
}

impl<'de> de::EnumAccess<'de> for EnumAccess<'de> {
    type Error = ValueError;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant)>
    where
        V: de::DeserializeSeed<'de>,
    {
        let val = seed.deserialize(self.name.clone().into_deserializer())?;
        Ok((val, self))
    }
}

impl<'de> de::VariantAccess<'de> for EnumAccess<'de> {
    type Error = ValueError;

    fn unit_variant(mut self) -> Result<()> {
        de::Deserialize::deserialize(&mut self.value_deserializer)
    }

    fn newtype_variant_seed<V>(mut self, seed: V) -> Result<V::Value>
    where
        V: de::DeserializeSeed<'de>,
    {
        seed.deserialize(&mut self.value_deserializer)
    }

    fn tuple_variant<V>(mut self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(&mut self.value_deserializer, visitor)
    }

    fn struct_variant<V>(mut self, fields: &'static [&'static str], visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        de::Deserializer::deserialize_struct(&mut self.value_deserializer, "", fields, visitor)
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::{Value, from_value};

    #[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
    enum UnitVariants {
        A,
        B,
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
    enum TupleEnum {
        A(i32),
        B(String, i32),
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
    enum StructEnum {
        A { number: i32 },
        B { text: String },
    }

    #[test]
    fn test_deserialize_unit_enum() {
        let value = Value::String("A".into());
        let deserialized = from_value::<UnitVariants>(&value).expect("deserialization failed");
        assert_eq!(deserialized, UnitVariants::A);

        let value = Value::String("B".into());
        let deserialized = from_value::<UnitVariants>(&value).expect("deserialization failed");
        assert_eq!(deserialized, UnitVariants::B);
    }

    #[test]
    fn test_deserialize_tuple_enum() {
        let value = json_value!({ "A": 42 });
        let deserialized = from_value::<TupleEnum>(&value).expect("deserialization failed");
        assert_eq!(deserialized, TupleEnum::A(42));

        let value = json_value!({ "B": [ "text", 1337 ] });
        let deserialized = from_value::<TupleEnum>(&value).expect("deserialization failed");
        if let TupleEnum::B(s, i) = deserialized {
            assert_eq!(s, "text");
            assert_eq!(i, 1337);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn test_deserialize_struct_enum() {
        let value = json_value!({ "A": { "number": 42 } });
        let deserialized = from_value::<StructEnum>(&value).expect("deserialization failed");
        assert_eq!(deserialized, StructEnum::A { number: 42 });

        let value = json_value!({ "B": { "text": "test" } });
        let deserialized = from_value::<StructEnum>(&value).expect("deserialization failed");
        assert_eq!(
            deserialized,
            StructEnum::B {
                text: "test".into()
            }
        );
    }

    /// `visit_str` made `from_value`'s lifetime parameter useless: no borrowed
    /// type could come out of it.
    #[test]
    fn a_string_can_be_borrowed_out_of_a_value() {
        let value = Value::String("borrowed".into());
        let borrowed: &str = from_value(&value).expect("a &str borrows from the Value");
        assert_eq!(borrowed, "borrowed");
    }

    /// A `Uint8List` used to be deserialized one element at a time through
    /// `visit_seq`, because `bytes`/`byte_buf` were forwarded to
    /// `deserialize_any`.
    #[test]
    fn a_byte_list_is_handed_over_as_one_borrowed_slice() {
        /// Asks for `deserialize_bytes` the way `serde_bytes` does, and accepts
        /// nothing but a borrowed slice.
        struct Borrowed<'a>(&'a [u8]);

        impl<'de> Deserialize<'de> for Borrowed<'de> {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct BytesVisitor;

                impl<'de> serde::de::Visitor<'de> for BytesVisitor {
                    type Value = &'de [u8];

                    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                        f.write_str("borrowed bytes")
                    }

                    fn visit_borrowed_bytes<E>(
                        self,
                        v: &'de [u8],
                    ) -> std::result::Result<&'de [u8], E>
                    where
                        E: serde::de::Error,
                    {
                        Ok(v)
                    }
                }

                deserializer.deserialize_bytes(BytesVisitor).map(Borrowed)
            }
        }

        let value = Value::U8List(vec![1, 2, 3]);
        let bytes: Borrowed<'_> = from_value(&value).expect("the bytes borrow from the Value");
        assert_eq!(bytes.0, [1, 2, 3]);
    }

    /// A `Float32List` has to reach plugin code; it used to fall into
    /// `SeqAccess`'s catch-all and come back as "value is not a list".
    #[test]
    fn a_float32_list_deserializes_as_a_sequence() {
        let value = Value::F32List(vec![1.5, 2.5]);
        let floats: Vec<f32> = from_value(&value).expect("an F32List deserializes");
        assert_eq!(floats, [1.5, 2.5]);
    }

    #[test]
    fn a_map_deserializes_into_a_struct() {
        #[derive(Deserialize, Debug, PartialEq, Eq)]
        struct Args {
            name: String,
            count: i32,
        }

        let value = json_value!({ "name": "n", "count": 3 });
        assert_eq!(
            from_value::<Args>(&value).expect("a map deserializes"),
            Args {
                name: "n".into(),
                count: 3,
            }
        );
    }

    #[test]
    fn a_value_of_the_wrong_shape_is_an_error_not_a_panic() {
        assert!(from_value::<Vec<i32>>(&Value::I32(1)).is_err());
        assert!(from_value::<bool>(&Value::String("x".into())).is_err());
        assert!(from_value::<UnitVariants>(&Value::Null).is_err());
    }
}
