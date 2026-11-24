//! A serde serializer that builds a [`Value`] directly.
//!
//! The counterpart of [`Deserializer`](super::Deserializer). It replaces a
//! round trip through `serde_json::Value`, which cost an outgoing
//! platform-channel call two trees instead of one and lost information on the
//! way:
//!
//! * `serde_json` has no byte-string type, so a `Vec<u8>` arrived as a
//!   [`Value::List`] of integers. Dart's standard codec writes that as nine
//!   bytes per byte and hands it to Dart as a `List<int>` rather than a
//!   `Uint8List`.
//! * `serde_json` turns a non-finite `f64` into `null`, so NaN and the
//!   infinities reached Dart as `null`. The standard codec writes raw IEEE-754
//!   bits and Dart's `double` has all three, so there is nothing to lose here
//!   and this serializer keeps them.
//!
//! # Which sequences become typed lists
//!
//! A sequence narrows to a typed list exactly when its serde element type has
//! no [`Value`] scalar of its own: `u8` becomes a [`Value::U8List`] and `f32`
//! a [`Value::F32List`]. `i32`, `i64` and `f64` do have one, so a sequence of
//! them stays a [`Value::List`] — narrowing those would mean a hand-built
//! `Value::List(vec![Value::I32(1)])` silently turning into a Dart
//! `Int32List`, because serde cannot tell it apart from `vec![1_i32]`.
//!
//! An *empty* sequence cannot be narrowed at all, because serde reports the
//! element type only through the elements themselves: `Vec::<u8>::new()`
//! becomes an empty [`Value::List`], not an empty [`Value::U8List`]. Code that
//! needs the `Uint8List` tag for a possibly-empty buffer has to build the
//! [`Value`] itself, or go through `serde_bytes`, whose `serialize_bytes` call
//! carries the type even when the buffer is empty.

use std::collections::HashMap;
use std::fmt::Display;
use std::mem;

use serde::ser::{self, Impossible, Serialize};

use super::{Error, Value};

type Result<T> = std::result::Result<T, Error>;

/// Serialize `value` into a [`Value`].
///
/// # Errors
///
/// Fails if `value` has no [`Value`] representation — see [`enum@Error`].
pub(super) fn to_value<T>(value: T) -> Result<Value>
where
    T: Serialize,
{
    value.serialize(Serializer).map(Node::into_value)
}

/// What one `serialize_*` call produced.
///
/// Two of Dart's type tags have no serde counterpart: a `Uint8List` and a
/// `Float32List` are, to serde, nothing but sequences of `u8` and of `f32`.
/// [`SeqBuilder`] therefore has to know which `serialize_*` method each
/// element went through, and a [`Value`] cannot carry that — `u8` and `f32`
/// widen to [`Value::I32`] and [`Value::F64`] the moment they become one,
/// because Dart has no narrower scalar than `int` and `double`.
#[derive(Debug)]
enum Node {
    /// A value already in its final shape.
    Value(Value),
    /// A `u8`, which is a [`Value::I32`] on its own and a [`Value::U8List`]
    /// element inside a sequence.
    U8(u8),
    /// An `f32`, likewise for [`Value::F32List`].
    F32(f32),
}

impl Node {
    /// Widen to the [`Value`] this node stands for outside a sequence.
    fn into_value(self) -> Value {
        match self {
            Node::Value(value) => value,
            Node::U8(byte) => Value::I32(i32::from(byte)),
            Node::F32(float) => Value::F64(f64::from(float)),
        }
    }
}

/// The serializer itself: a unit struct, since a [`Value`] is built bottom-up
/// out of the sub-serializers below rather than appended to a buffer.
///
/// Private on purpose. Its `Ok` type is [`Node`], which is an implementation
/// detail of the narrowing described in the module documentation, so exposing
/// the serializer would mean exposing that too.
struct Serializer;

impl ser::Serializer for Serializer {
    type Ok = Node;
    type Error = Error;

    type SerializeSeq = SerializeSeq;
    type SerializeTuple = SerializeSeq;
    type SerializeTupleStruct = SerializeSeq;
    type SerializeTupleVariant = SerializeTupleVariant;
    type SerializeMap = SerializeMap;
    type SerializeStruct = SerializeMap;
    type SerializeStructVariant = SerializeStructVariant;

    fn serialize_bool(self, v: bool) -> Result<Node> {
        Ok(Node::Value(Value::Boolean(v)))
    }

    fn serialize_i8(self, v: i8) -> Result<Node> {
        Ok(Node::Value(Value::I32(i32::from(v))))
    }

    fn serialize_i16(self, v: i16) -> Result<Node> {
        Ok(Node::Value(Value::I32(i32::from(v))))
    }

    fn serialize_i32(self, v: i32) -> Result<Node> {
        Ok(Node::Value(Value::I32(v)))
    }

    fn serialize_i64(self, v: i64) -> Result<Node> {
        Ok(Node::Value(Value::I64(v)))
    }

    fn serialize_i128(self, v: i128) -> Result<Node> {
        i64::try_from(v)
            .map(|v| Node::Value(Value::I64(v)))
            .map_err(|_| Error::NumberOutOfRange)
    }

    /// A `u8` keeps its type until [`SeqBuilder`] has seen whether it is one
    /// of many, because a sequence of them is a `Uint8List`.
    fn serialize_u8(self, v: u8) -> Result<Node> {
        Ok(Node::U8(v))
    }

    fn serialize_u16(self, v: u16) -> Result<Node> {
        Ok(Node::Value(Value::I32(i32::from(v))))
    }

    fn serialize_u32(self, v: u32) -> Result<Node> {
        Ok(Node::Value(Value::I64(i64::from(v))))
    }

    fn serialize_u64(self, v: u64) -> Result<Node> {
        i64::try_from(v)
            .map(|v| Node::Value(Value::I64(v)))
            .map_err(|_| Error::NumberOutOfRange)
    }

    fn serialize_u128(self, v: u128) -> Result<Node> {
        i64::try_from(v)
            .map(|v| Node::Value(Value::I64(v)))
            .map_err(|_| Error::NumberOutOfRange)
    }

    /// An `f32` keeps its type until [`SeqBuilder`] has seen whether it is one
    /// of many, because a sequence of them is a `Float32List`.
    ///
    /// NaN and the infinities are passed through: the standard codec writes
    /// raw IEEE-754 bits and Dart's `double` has all three.
    fn serialize_f32(self, v: f32) -> Result<Node> {
        Ok(Node::F32(v))
    }

    /// See [`serialize_f32`](Self::serialize_f32) on non-finite values.
    fn serialize_f64(self, v: f64) -> Result<Node> {
        Ok(Node::Value(Value::F64(v)))
    }

    /// Dart has no character type, so a `char` is a one-character string —
    /// which is also what `serde_json` did.
    fn serialize_char(self, v: char) -> Result<Node> {
        Ok(Node::Value(Value::String(v.to_string())))
    }

    fn serialize_str(self, v: &str) -> Result<Node> {
        Ok(Node::Value(Value::String(v.to_owned())))
    }

    /// Only reached through a `Serialize` impl that asks for it, such as
    /// `serde_bytes`: serde's own `Vec<u8>` impl goes through
    /// [`serialize_seq`](Self::serialize_seq). It is the one path that can
    /// produce an *empty* [`Value::U8List`].
    fn serialize_bytes(self, v: &[u8]) -> Result<Node> {
        Ok(Node::Value(Value::U8List(v.to_vec())))
    }

    fn serialize_none(self) -> Result<Node> {
        Ok(Node::Value(Value::Null))
    }

    fn serialize_some<T>(self, value: &T) -> Result<Node>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Node> {
        Ok(Node::Value(Value::Null))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Node> {
        Ok(Node::Value(Value::Null))
    }

    /// A unit variant is its own name, which is how
    /// [`Deserializer`](super::Deserializer) reads one back.
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Node> {
        Ok(Node::Value(Value::String(variant.to_owned())))
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<Node>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    /// A non-unit variant is a single-entry map of variant name to body, the
    /// externally tagged representation `serde_json` uses and
    /// [`Deserializer`](super::Deserializer) expects.
    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Node>
    where
        T: ?Sized + Serialize,
    {
        Ok(Node::Value(wrap_variant(
            variant,
            value.serialize(Serializer)?.into_value(),
        )))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<SerializeSeq> {
        Ok(SerializeSeq {
            builder: SeqBuilder::Empty(len.unwrap_or(0)),
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<SerializeSeq> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(self, _name: &'static str, len: usize) -> Result<SerializeSeq> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<SerializeTupleVariant> {
        Ok(SerializeTupleVariant {
            variant,
            builder: SeqBuilder::Empty(len),
        })
    }

    fn serialize_map(self, len: Option<usize>) -> Result<SerializeMap> {
        Ok(SerializeMap {
            map: HashMap::with_capacity(len.unwrap_or(0)),
            key: None,
        })
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<SerializeMap> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<SerializeStructVariant> {
        Ok(SerializeStructVariant {
            variant,
            map: HashMap::with_capacity(len),
        })
    }

    /// `true`, which is what `serde_json` reported: a type with both a compact
    /// and a text form — an `IpAddr`, a `SystemTime` — keeps the text form it
    /// had while `to_value` still went through JSON, and
    /// [`Deserializer`](super::Deserializer) reads it back the same way.
    fn is_human_readable(&self) -> bool {
        true
    }
}

/// Wrap `body` as the externally tagged representation of `variant`.
fn wrap_variant(variant: &'static str, body: Value) -> Value {
    Value::Map(HashMap::from([(variant.to_owned(), body)]))
}

/// A sequence under construction.
///
/// Undecided until the first element arrives, then narrowed to the typed list
/// that element calls for, and widened back to a plain [`Value::List`] as soon
/// as an element does not fit it. See the module documentation for why only
/// `u8` and `f32` narrow.
#[derive(Debug)]
enum SeqBuilder {
    /// No element yet. Carries serde's length hint, which comes from the Rust
    /// value being serialized rather than off the wire and is therefore safe
    /// to allocate on.
    Empty(usize),
    /// Every element so far arrived through `serialize_u8`.
    U8(Vec<u8>),
    /// Every element so far arrived through `serialize_f32`.
    F32(Vec<f32>),
    /// Anything else.
    Mixed(Vec<Value>),
}

impl SeqBuilder {
    fn push(&mut self, node: Node) {
        // Replaced rather than borrowed so the arms below can move the
        // partially built list out; every arm puts a list back.
        match (mem::replace(self, Self::Empty(0)), node) {
            (Self::Empty(capacity), Node::U8(byte)) => {
                let mut list = Vec::with_capacity(capacity);
                list.push(byte);
                *self = Self::U8(list);
            }
            (Self::Empty(capacity), Node::F32(float)) => {
                let mut list = Vec::with_capacity(capacity);
                list.push(float);
                *self = Self::F32(list);
            }
            (Self::U8(mut list), Node::U8(byte)) => {
                list.push(byte);
                *self = Self::U8(list);
            }
            (Self::F32(mut list), Node::F32(float)) => {
                list.push(float);
                *self = Self::F32(list);
            }
            (Self::Mixed(mut list), node) => {
                list.push(node.into_value());
                *self = Self::Mixed(list);
            }
            // Either the first element does not narrow, or this one broke the
            // narrowing the earlier ones established: widen what was collected
            // so far and carry on as a plain list.
            (collected, node) => {
                let mut list = collected.into_values();
                list.push(node.into_value());
                *self = Self::Mixed(list);
            }
        }
    }

    /// Widen every element collected so far to a [`Value`].
    fn into_values(self) -> Vec<Value> {
        match self {
            Self::Empty(capacity) => Vec::with_capacity(capacity),
            Self::U8(list) => list
                .into_iter()
                .map(Node::U8)
                .map(Node::into_value)
                .collect(),
            Self::F32(list) => list
                .into_iter()
                .map(Node::F32)
                .map(Node::into_value)
                .collect(),
            Self::Mixed(list) => list,
        }
    }

    fn finish(self) -> Value {
        match self {
            Self::U8(list) => Value::U8List(list),
            Self::F32(list) => Value::F32List(list),
            // An empty sequence never narrowed, because serde reports the
            // element type only through the elements.
            Self::Empty(_) => Value::List(Vec::new()),
            Self::Mixed(list) => Value::List(list),
        }
    }
}

/// Collects a sequence, a tuple or a tuple struct.
struct SerializeSeq {
    builder: SeqBuilder,
}

impl ser::SerializeSeq for SerializeSeq {
    type Ok = Node;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.builder.push(value.serialize(Serializer)?);
        Ok(())
    }

    fn end(self) -> Result<Node> {
        Ok(Node::Value(self.builder.finish()))
    }
}

impl ser::SerializeTuple for SerializeSeq {
    type Ok = Node;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Node> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleStruct for SerializeSeq {
    type Ok = Node;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Node> {
        ser::SerializeSeq::end(self)
    }
}

/// Collects a tuple enum variant, which ends as `{variant: [fields]}`.
struct SerializeTupleVariant {
    variant: &'static str,
    builder: SeqBuilder,
}

impl ser::SerializeTupleVariant for SerializeTupleVariant {
    type Ok = Node;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.builder.push(value.serialize(Serializer)?);
        Ok(())
    }

    fn end(self) -> Result<Node> {
        Ok(Node::Value(wrap_variant(
            self.variant,
            self.builder.finish(),
        )))
    }
}

/// Collects a map or a struct.
struct SerializeMap {
    map: HashMap<String, Value>,
    /// The key `serialize_key` produced, waiting for its value. serde always
    /// pairs the two, but a hand-written `Serialize` impl can get it wrong, so
    /// `serialize_value` reports a missing key rather than panicking.
    key: Option<String>,
}

impl ser::SerializeMap for SerializeMap {
    type Ok = Node;
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.key = Some(key.serialize(MapKeySerializer)?);
        Ok(())
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        let key = self
            .key
            .take()
            .ok_or_else(|| ser::Error::custom("serialize_value called before serialize_key"))?;
        self.map
            .insert(key, value.serialize(Serializer)?.into_value());
        Ok(())
    }

    fn end(self) -> Result<Node> {
        Ok(Node::Value(Value::Map(self.map)))
    }
}

impl ser::SerializeStruct for SerializeMap {
    type Ok = Node;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.map
            .insert(key.to_owned(), value.serialize(Serializer)?.into_value());
        Ok(())
    }

    fn end(self) -> Result<Node> {
        ser::SerializeMap::end(self)
    }
}

/// Collects a struct enum variant, which ends as `{variant: {fields}}`.
struct SerializeStructVariant {
    variant: &'static str,
    map: HashMap<String, Value>,
}

impl ser::SerializeStructVariant for SerializeStructVariant {
    type Ok = Node;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.map
            .insert(key.to_owned(), value.serialize(Serializer)?.into_value());
        Ok(())
    }

    fn end(self) -> Result<Node> {
        Ok(Node::Value(wrap_variant(
            self.variant,
            Value::Map(self.map),
        )))
    }
}

/// Serializes a map key, which a [`Value::Map`] can only hold as a `String`.
///
/// Dart's standard codec allows any value as a map key, but [`Value::Map`] is
/// a `HashMap<String, Value>`, so a key has to be rendered as text. Strings,
/// `char`s, integers, `bool`s and unit enum variants have an unambiguous text
/// form and are accepted — integers in particular, because `serde_json`
/// accepted them in this position and a `HashMap<i32, _>` has to keep working.
/// Floats are not: `1.0` and `1` would collide as keys, and NaN has no
/// meaningful spelling.
struct MapKeySerializer;

/// The error every rejected key shares.
fn non_string_key<T>() -> Result<T> {
    Err(Error::NonStringKey)
}

impl ser::Serializer for MapKeySerializer {
    type Ok = String;
    type Error = Error;

    type SerializeSeq = Impossible<String, Error>;
    type SerializeTuple = Impossible<String, Error>;
    type SerializeTupleStruct = Impossible<String, Error>;
    type SerializeTupleVariant = Impossible<String, Error>;
    type SerializeMap = Impossible<String, Error>;
    type SerializeStruct = Impossible<String, Error>;
    type SerializeStructVariant = Impossible<String, Error>;

    fn serialize_bool(self, v: bool) -> Result<String> {
        Ok(v.to_string())
    }

    fn serialize_i8(self, v: i8) -> Result<String> {
        Ok(v.to_string())
    }

    fn serialize_i16(self, v: i16) -> Result<String> {
        Ok(v.to_string())
    }

    fn serialize_i32(self, v: i32) -> Result<String> {
        Ok(v.to_string())
    }

    fn serialize_i64(self, v: i64) -> Result<String> {
        Ok(v.to_string())
    }

    fn serialize_i128(self, v: i128) -> Result<String> {
        Ok(v.to_string())
    }

    fn serialize_u8(self, v: u8) -> Result<String> {
        Ok(v.to_string())
    }

    fn serialize_u16(self, v: u16) -> Result<String> {
        Ok(v.to_string())
    }

    fn serialize_u32(self, v: u32) -> Result<String> {
        Ok(v.to_string())
    }

    fn serialize_u64(self, v: u64) -> Result<String> {
        Ok(v.to_string())
    }

    fn serialize_u128(self, v: u128) -> Result<String> {
        Ok(v.to_string())
    }

    fn serialize_f32(self, _v: f32) -> Result<String> {
        non_string_key()
    }

    fn serialize_f64(self, _v: f64) -> Result<String> {
        non_string_key()
    }

    fn serialize_char(self, v: char) -> Result<String> {
        Ok(v.to_string())
    }

    fn serialize_str(self, v: &str) -> Result<String> {
        Ok(v.to_owned())
    }

    fn serialize_bytes(self, _v: &[u8]) -> Result<String> {
        non_string_key()
    }

    fn serialize_none(self) -> Result<String> {
        non_string_key()
    }

    fn serialize_some<T>(self, _value: &T) -> Result<String>
    where
        T: ?Sized + Serialize,
    {
        non_string_key()
    }

    fn serialize_unit(self) -> Result<String> {
        non_string_key()
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<String> {
        non_string_key()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<String> {
        Ok(variant.to_owned())
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<String>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<String>
    where
        T: ?Sized + Serialize,
    {
        non_string_key()
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        non_string_key()
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        non_string_key()
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        non_string_key()
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        non_string_key()
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        non_string_key()
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        non_string_key()
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        non_string_key()
    }

    /// A key whose `Serialize` impl writes itself with `Display` — a `Path`,
    /// an `IpAddr` — is text, so it is a usable key.
    fn collect_str<T>(self, value: &T) -> Result<String>
    where
        T: ?Sized + Display,
    {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::super::from_value;
    use super::{Error, Value, to_value};
    use crate::codec::{MessageCodec, STANDARD_CODEC};
    use std::collections::HashMap;

    /// The point of the whole module: a `Vec<u8>` used to cost nine bytes per
    /// byte on the wire and arrive in Dart as a `List<int>`.
    #[test]
    fn a_byte_sequence_becomes_a_uint8_list() {
        assert_eq!(
            to_value(vec![1_u8, 2, 3]).expect("representable"),
            Value::U8List(vec![1, 2, 3])
        );
        // An array serializes as a tuple, not a seq, and has to narrow too —
        // a `[u8; 16]` UUID is the common case.
        assert_eq!(
            to_value([1_u8, 2, 3]).expect("representable"),
            Value::U8List(vec![1, 2, 3])
        );
        // ... which is five bytes on the wire instead of twenty-three.
        assert_eq!(
            STANDARD_CODEC.encode_message(&to_value(vec![1_u8, 2, 3]).expect("representable")),
            [8, 3, 1, 2, 3]
        );
    }

    #[test]
    fn a_float32_sequence_becomes_a_float32_list() {
        assert_eq!(
            to_value(vec![1.5_f32, 2.5]).expect("representable"),
            Value::F32List(vec![1.5, 2.5])
        );
    }

    /// serde reports the element type only through the elements, so an empty
    /// sequence cannot be narrowed. Documented on `to_value`, pinned here.
    #[test]
    fn an_empty_sequence_stays_a_plain_list() {
        assert_eq!(
            to_value(Vec::<u8>::new()).expect("representable"),
            Value::List(vec![])
        );
        assert_eq!(
            to_value(Vec::<f32>::new()).expect("representable"),
            Value::List(vec![])
        );
    }

    /// `serialize_bytes` carries the type even for an empty buffer, which is
    /// the escape hatch for the case above.
    #[test]
    fn serialize_bytes_narrows_even_when_the_buffer_is_empty() {
        struct Bytes(&'static [u8]);

        impl Serialize for Bytes {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_bytes(self.0)
            }
        }

        assert_eq!(
            to_value(Bytes(&[])).expect("representable"),
            Value::U8List(vec![])
        );
        assert_eq!(
            to_value(Bytes(&[7])).expect("representable"),
            Value::U8List(vec![7])
        );
    }

    #[test]
    fn a_sequence_that_breaks_its_narrowing_widens_to_a_plain_list() {
        // u8 first, then something else.
        assert_eq!(
            to_value((1_u8, "x")).expect("representable"),
            Value::List(vec![Value::I32(1), Value::String("x".into())])
        );
        // Something else first, then u8.
        assert_eq!(
            to_value(("x", 1_u8)).expect("representable"),
            Value::List(vec![Value::String("x".into()), Value::I32(1)])
        );
        // Two narrowings that disagree.
        assert_eq!(
            to_value((1_u8, 1.5_f32)).expect("representable"),
            Value::List(vec![Value::I32(1), Value::F64(1.5)])
        );
    }

    #[test]
    fn a_lone_u8_or_f32_widens_to_a_dart_scalar() {
        assert_eq!(to_value(7_u8).expect("representable"), Value::I32(7));
        assert_eq!(to_value(1.5_f32).expect("representable"), Value::F64(1.5));
    }

    /// `serde_json` turned every non-finite float into `null`, so NaN reached
    /// Dart as `null` on a standard-codec channel that could have carried it.
    #[test]
    fn non_finite_floats_are_not_silently_nulled() {
        for float in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let Value::F64(decoded) = to_value(float).expect("representable") else {
                panic!("{float} did not serialize as an F64");
            };
            assert_eq!(decoded.is_nan(), float.is_nan());
            assert_eq!(decoded.to_bits(), float.to_bits(), "{float}");
        }
        let Value::F64(nan) = to_value(f32::NAN).expect("representable") else {
            panic!("an f32 NaN did not serialize as an F64");
        };
        assert!(nan.is_nan());
        // And through the standard codec, which writes raw IEEE-754 bits.
        let encoded = STANDARD_CODEC.encode_message(&Value::F64(f64::INFINITY));
        assert_eq!(
            STANDARD_CODEC.decode_message(&encoded).expect("decodes"),
            Value::F64(f64::INFINITY)
        );
    }

    #[test]
    fn integers_map_to_the_narrowest_dart_int_that_holds_them() {
        assert_eq!(to_value(-1_i8).expect("representable"), Value::I32(-1));
        assert_eq!(to_value(-1_i16).expect("representable"), Value::I32(-1));
        assert_eq!(to_value(-1_i32).expect("representable"), Value::I32(-1));
        assert_eq!(to_value(7_u16).expect("representable"), Value::I32(7));
        assert_eq!(to_value(-1_i64).expect("representable"), Value::I64(-1));
        assert_eq!(
            to_value(u32::MAX).expect("representable"),
            Value::I64(i64::from(u32::MAX))
        );
        assert_eq!(to_value(7_i128).expect("representable"), Value::I64(7));
        assert!(matches!(
            to_value(i128::MAX).expect_err("too large"),
            Error::NumberOutOfRange
        ));
        assert!(matches!(
            to_value(u128::MAX).expect_err("too large"),
            Error::NumberOutOfRange
        ));
    }

    #[test]
    fn scalars_and_absent_values() {
        assert_eq!(to_value(true).expect("representable"), Value::Boolean(true));
        assert_eq!(
            to_value('x').expect("representable"),
            Value::String("x".into())
        );
        assert_eq!(to_value(()).expect("representable"), Value::Null);
        assert_eq!(to_value(None::<i32>).expect("representable"), Value::Null);
        assert_eq!(to_value(Some(1_i32)).expect("representable"), Value::I32(1));
    }

    /// `serde_json` rendered an integer map key as a decimal string, so a
    /// `HashMap<i32, _>` worked; it has to keep working.
    #[test]
    fn map_keys_with_a_text_form_are_accepted() {
        assert_eq!(
            to_value(HashMap::from([(7_i32, 1_i32)])).expect("representable"),
            Value::Map(HashMap::from([("7".to_owned(), Value::I32(1))]))
        );
        assert_eq!(
            to_value(HashMap::from([('k', 1_i32)])).expect("representable"),
            Value::Map(HashMap::from([("k".to_owned(), Value::I32(1))]))
        );
        assert_eq!(
            to_value(HashMap::from([("k", 1_i32)])).expect("representable"),
            Value::Map(HashMap::from([("k".to_owned(), Value::I32(1))]))
        );
        // `Ipv4Addr` writes itself through `collect_str`.
        assert_eq!(
            to_value(HashMap::from([(std::net::Ipv4Addr::LOCALHOST, 1_i32)]))
                .expect("representable"),
            Value::Map(HashMap::from([("127.0.0.1".to_owned(), Value::I32(1))]))
        );
    }

    /// `1.0` and `1` would collide as keys and NaN has no spelling, so a float
    /// key is an error — as it was through `serde_json`.
    #[test]
    fn a_map_key_without_a_text_form_is_rejected() {
        /// A key that writes itself as a float, and a map that uses it.
        struct FloatKey;

        impl Serialize for FloatKey {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_f64(1.0)
            }
        }

        struct FloatKeyedMap;

        impl Serialize for FloatKeyedMap {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeMap;
                let mut map = s.serialize_map(Some(1))?;
                map.serialize_entry(&FloatKey, &1_i32)?;
                map.end()
            }
        }

        assert!(matches!(
            to_value(FloatKeyedMap).expect_err("a float key is rejected"),
            Error::NonStringKey
        ));
        // A tuple key is the case plugin code really hit.
        assert!(matches!(
            to_value(HashMap::from([((1_i32, 2_i32), "pair")])).expect_err("a tuple key too"),
            Error::NonStringKey
        ));
    }

    #[test]
    fn a_struct_becomes_a_map_and_deserializes_back() {
        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct Args {
            name: String,
            count: i32,
            #[serde(with = "serde_bytes_like")]
            blob: Vec<u8>,
        }

        /// A `with` module that routes through `serialize_bytes`, the way
        /// `serde_bytes` does.
        mod serde_bytes_like {
            pub fn serialize<S: serde::Serializer>(value: &[u8], s: S) -> Result<S::Ok, S::Error> {
                s.serialize_bytes(value)
            }

            pub fn deserialize<'de, D: serde::Deserializer<'de>>(
                d: D,
            ) -> Result<Vec<u8>, D::Error> {
                serde::Deserialize::deserialize(d)
            }
        }

        let args = Args {
            name: "n".into(),
            count: 3,
            blob: vec![9, 8],
        };
        let value = to_value(&args).expect("representable");
        let Value::Map(map) = &value else {
            panic!("a struct must serialize as a map, got {value:?}");
        };
        assert_eq!(map.get("count"), Some(&Value::I32(3)));
        assert_eq!(map.get("blob"), Some(&Value::U8List(vec![9, 8])));
        assert_eq!(from_value::<Args>(&value).expect("deserializes"), args);
    }

    /// Every enum shape has to come back out through
    /// [`Deserializer`](super::super::Deserializer), which is the only reader
    /// of what this serializer writes.
    #[test]
    fn every_enum_shape_round_trips_through_the_deserializer() {
        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        enum Shape {
            Unit,
            Newtype(i32),
            Tuple(String, i32),
            Struct { number: i32 },
        }

        for shape in [
            Shape::Unit,
            Shape::Newtype(42),
            Shape::Tuple("text".into(), 1337),
            Shape::Struct { number: 42 },
        ] {
            let value = to_value(&shape).expect("representable");
            assert_eq!(
                from_value::<Shape>(&value).expect("deserializes"),
                shape,
                "round-tripping {shape:?} through {value:?}"
            );
        }

        // The wire shapes themselves, since Dart sees these.
        assert_eq!(
            to_value(Shape::Unit).expect("representable"),
            Value::String("Unit".into())
        );
        assert_eq!(
            to_value(Shape::Newtype(1)).expect("representable"),
            Value::Map(HashMap::from([("Newtype".to_owned(), Value::I32(1))]))
        );
        assert_eq!(
            to_value(Shape::Tuple("a".into(), 1)).expect("representable"),
            Value::Map(HashMap::from([(
                "Tuple".to_owned(),
                Value::List(vec![Value::String("a".into()), Value::I32(1)])
            )]))
        );
        assert_eq!(
            to_value(Shape::Struct { number: 1 }).expect("representable"),
            Value::Map(HashMap::from([(
                "Struct".to_owned(),
                Value::Map(HashMap::from([("number".to_owned(), Value::I32(1))]))
            )]))
        );
    }

    #[test]
    fn a_newtype_and_tuple_struct_keep_serde_jsons_shapes() {
        #[derive(Serialize)]
        struct Newtype(i32);
        #[derive(Serialize)]
        struct Tuple(i32, i32);
        #[derive(Serialize)]
        struct Unit;

        assert_eq!(to_value(Newtype(1)).expect("representable"), Value::I32(1));
        assert_eq!(
            to_value(Tuple(1, 2)).expect("representable"),
            Value::List(vec![Value::I32(1), Value::I32(2)])
        );
        assert_eq!(to_value(Unit).expect("representable"), Value::Null);
    }

    /// A `Serialize` impl of its own can fail, and the failure has to come out
    /// as an error rather than a panic: `to_value` runs inside the engine's
    /// platform-message callback.
    #[test]
    fn a_failing_serialize_impl_is_reported() {
        struct Fails;

        impl Serialize for Fails {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("no"))
            }
        }

        let error = to_value(Fails).expect_err("the impl failed");
        assert!(
            matches!(error, Error::Message(ref msg) if msg == "no"),
            "{error:?}"
        );
    }

    /// Nested containers are where a hand-written serializer usually goes
    /// wrong, and where the allocation saving is largest.
    #[test]
    fn nested_containers_keep_their_shape() {
        let value = to_value(vec![
            HashMap::from([("bytes", vec![1_u8, 2])]),
            HashMap::from([("bytes", vec![3_u8])]),
        ])
        .expect("representable");
        assert_eq!(
            value,
            Value::List(vec![
                Value::Map(HashMap::from([(
                    "bytes".to_owned(),
                    Value::U8List(vec![1, 2])
                )])),
                Value::Map(HashMap::from([(
                    "bytes".to_owned(),
                    Value::U8List(vec![3])
                )])),
            ])
        );
    }
}
