//! Dart's `StandardMethodCodec`: a compact binary format.
//!
//! Each value is a one-byte type tag, an optional size prefix and a payload.
//! Sizes below 254 are a single byte, 254 introduces a `u16` and 255 a `u32`.
//! Multi-byte payloads use the host's byte order (Dart writes `Endian.host`)
//! and typed lists are padded so their elements are aligned *relative to the
//! start of the message*, not to the start of the list.
//!
//! Decoding never panics and never indexes the buffer directly: every read goes
//! through [`Reader::take`], which bounds-checks against the buffer, so a
//! truncated or mismatched message is a [`DecodeError`].

use std::collections::HashMap;

use super::{DecodeError, MessageCodec, MethodCall, MethodCallResult, MethodCodec, Value};

const VALUE_NULL: u8 = 0;
const VALUE_TRUE: u8 = 1;
const VALUE_FALSE: u8 = 2;
const VALUE_INT32: u8 = 3;
const VALUE_INT64: u8 = 4;
const VALUE_LARGEINT: u8 = 5;
const VALUE_FLOAT64: u8 = 6;
const VALUE_STRING: u8 = 7;
const VALUE_UINT8LIST: u8 = 8;
const VALUE_INT32LIST: u8 = 9;
const VALUE_INT64LIST: u8 = 10;
const VALUE_FLOAT64LIST: u8 = 11;
const VALUE_LIST: u8 = 12;
const VALUE_MAP: u8 = 13;
const VALUE_FLOAT32LIST: u8 = 14;

/// The size prefix that introduces a `u16` length.
const SIZE_U16: u8 = 254;
/// The size prefix that introduces a `u32` length.
const SIZE_U32: u8 = 255;

/// A rough guess at a message size, to save the first few reallocations.
const WRITE_BUFFER_START_CAPACITY: usize = 64;

/// Dart's `StandardMethodCodec` / `StandardMessageCodec`.
#[derive(Debug)]
pub struct StandardMethodCodec;

/// The shared [`StandardMethodCodec`] instance to hand to a channel.
pub const STANDARD_CODEC: StandardMethodCodec = StandardMethodCodec {};

impl StandardMethodCodec {
    fn read_value(reader: &mut Reader) -> Result<Value, DecodeError> {
        let t = reader.read_u8()?;
        Ok(match t {
            VALUE_NULL => Value::Null,
            VALUE_FALSE => Value::Boolean(false),
            VALUE_TRUE => Value::Boolean(true),
            VALUE_INT32 => Value::I32(reader.read_i32()?),
            VALUE_INT64 => Value::I64(reader.read_i64()?),
            // A big integer is a length-prefixed hexadecimal string on the
            // wire. Dart's own `writeValue` never emits this tag, but the
            // protocol reserves it for codec subclasses.
            VALUE_LARGEINT | VALUE_STRING => {
                let len = reader.read_size()?;
                Value::String(reader.read_string(len)?)
            }
            VALUE_FLOAT64 => {
                reader.align_to(8);
                Value::F64(reader.read_f64()?)
            }
            VALUE_UINT8LIST => {
                let len = reader.read_size()?;
                Value::U8List(reader.read_u8_list(len)?)
            }
            VALUE_INT32LIST => {
                let len = reader.read_size()?;
                Value::I32List(reader.read_i32_list(len)?)
            }
            VALUE_INT64LIST => {
                let len = reader.read_size()?;
                Value::I64List(reader.read_i64_list(len)?)
            }
            VALUE_FLOAT32LIST => {
                let len = reader.read_size()?;
                Value::F32List(reader.read_f32_list(len)?)
            }
            VALUE_FLOAT64LIST => {
                let len = reader.read_size()?;
                Value::F64List(reader.read_f64_list(len)?)
            }
            VALUE_LIST => {
                let len = reader.read_size()?;
                // `len` comes off the wire, so it cannot be used to pre-size
                // the vector: the smallest element is one byte, but a bogus
                // length would still ask the allocator for gigabytes.
                let mut list = Vec::new();
                for _ in 0..len {
                    list.push(Self::read_value(reader)?);
                }
                Value::List(list)
            }
            VALUE_MAP => {
                let len = reader.read_size()?;
                let mut map = HashMap::new();
                for _ in 0..len {
                    let Value::String(key) = Self::read_value(reader)? else {
                        return Err(DecodeError::WrongType {
                            expected: "a string map key",
                        });
                    };
                    map.insert(key, Self::read_value(reader)?);
                }
                Value::Map(map)
            }
            // Anything else means the two sides disagree about the codec, or
            // the format grew a tag this crate does not know yet. Returning
            // `Value::Null` would leave the size prefix and payload unread and
            // silently corrupt every later value in the message.
            unknown => return Err(DecodeError::UnknownType(unknown)),
        })
    }

    fn write_string(writer: &mut Writer, s: &str) {
        writer.write_u8(VALUE_STRING);
        writer.write_size(s.len());
        writer.write_bytes(s.as_bytes());
    }

    fn write_value(writer: &mut Writer, v: &Value) {
        match v {
            Value::Null => {
                writer.write_u8(VALUE_NULL);
            }
            Value::Boolean(v) => {
                writer.write_u8(if *v { VALUE_TRUE } else { VALUE_FALSE });
            }
            Value::I32(n) => {
                writer.write_u8(VALUE_INT32);
                writer.write_i32(*n);
            }
            Value::I64(n) => {
                writer.write_u8(VALUE_INT64);
                writer.write_i64(*n);
            }
            Value::F64(n) => {
                writer.write_u8(VALUE_FLOAT64);
                writer.align_to(8);
                writer.write_f64(*n);
            }
            Value::String(s) => {
                Self::write_string(writer, s);
            }
            Value::U8List(list) => {
                writer.write_u8(VALUE_UINT8LIST);
                writer.write_size(list.len());
                writer.write_bytes(list);
            }
            Value::I32List(list) => {
                writer.write_u8(VALUE_INT32LIST);
                writer.write_size(list.len());
                writer.align_to(4);
                for n in list {
                    writer.write_i32(*n);
                }
            }
            Value::I64List(list) => {
                writer.write_u8(VALUE_INT64LIST);
                writer.write_size(list.len());
                writer.align_to(8);
                for n in list {
                    writer.write_i64(*n);
                }
            }
            Value::F32List(list) => {
                writer.write_u8(VALUE_FLOAT32LIST);
                writer.write_size(list.len());
                writer.align_to(4);
                for n in list {
                    writer.write_u32(n.to_bits());
                }
            }
            Value::F64List(list) => {
                writer.write_u8(VALUE_FLOAT64LIST);
                writer.write_size(list.len());
                writer.align_to(8);
                for n in list {
                    writer.write_f64(*n);
                }
            }
            Value::List(list) => {
                writer.write_u8(VALUE_LIST);
                writer.write_size(list.len());
                for v in list {
                    Self::write_value(writer, v);
                }
            }
            Value::Map(map) => {
                writer.write_u8(VALUE_MAP);
                writer.write_size(map.len());
                for (k, v) in map {
                    Self::write_string(writer, k);
                    Self::write_value(writer, v);
                }
            }
        }
    }
}

impl MethodCodec for StandardMethodCodec {
    fn encode_method_call(&self, v: &MethodCall) -> Vec<u8> {
        let mut writer = Writer::with_capacity(v.method.len() + WRITE_BUFFER_START_CAPACITY);
        StandardMethodCodec::write_string(&mut writer, &v.method);
        StandardMethodCodec::write_value(&mut writer, &v.args);
        writer.0
    }

    fn decode_method_call(&self, buf: &[u8]) -> Result<MethodCall, DecodeError> {
        let mut reader = Reader::new(buf);
        let method = StandardMethodCodec::read_value(&mut reader)?;
        let args = StandardMethodCodec::read_value(&mut reader)?;

        match method {
            Value::String(method) => Ok(MethodCall { method, args }),
            _ => Err(DecodeError::WrongType {
                expected: "a string method name",
            }),
        }
    }

    fn encode_success_envelope(&self, result: &Value) -> Vec<u8> {
        let mut writer = Writer::with_capacity(WRITE_BUFFER_START_CAPACITY);
        writer.write_u8(0);
        StandardMethodCodec::write_value(&mut writer, result);
        writer.0
    }

    fn encode_error_envelope(&self, code: &str, message: &str, v: &Value) -> Vec<u8> {
        let mut writer =
            Writer::with_capacity(code.len() + message.len() + WRITE_BUFFER_START_CAPACITY);
        writer.write_u8(1);
        StandardMethodCodec::write_string(&mut writer, code);
        StandardMethodCodec::write_string(&mut writer, message);
        StandardMethodCodec::write_value(&mut writer, v);
        writer.0
    }

    fn decode_envelope(&self, buf: &[u8]) -> Result<MethodCallResult, DecodeError> {
        // An empty envelope is how the framework says "no plugin handled this".
        if buf.is_empty() {
            return Ok(MethodCallResult::NotImplemented);
        }

        let mut reader = Reader::new(buf);
        match reader.read_u8()? {
            0 => Ok(MethodCallResult::Ok(StandardMethodCodec::read_value(
                &mut reader,
            )?)),
            1 => {
                let code = StandardMethodCodec::read_value(&mut reader)?;
                let message = StandardMethodCodec::read_value(&mut reader)?;
                let details = StandardMethodCodec::read_value(&mut reader)?;
                Ok(MethodCallResult::Err {
                    code: match code {
                        Value::String(s) => s,
                        _ => String::new(),
                    },
                    message: match message {
                        Value::String(s) => s,
                        _ => String::new(),
                    },
                    details,
                })
            }
            _ => Err(DecodeError::WrongType {
                expected: "an envelope starting with 0 (success) or 1 (error)",
            }),
        }
    }
}

impl MessageCodec for StandardMethodCodec {
    fn encode_message(&self, v: &Value) -> Vec<u8> {
        let mut writer = Writer::with_capacity(WRITE_BUFFER_START_CAPACITY);
        StandardMethodCodec::write_value(&mut writer, v);
        writer.0
    }

    fn decode_message(&self, buf: &[u8]) -> Result<Value, DecodeError> {
        let mut reader = Reader::new(buf);
        StandardMethodCodec::read_value(&mut reader)
    }
}

/// A bounds-checked cursor over a message the engine handed us.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    /// The next `len` bytes, or [`DecodeError::Truncated`] if the buffer is
    /// shorter than that.
    ///
    /// This is the only place the buffer is indexed, so no wire-supplied length
    /// can reach past its end.
    fn take(&mut self, len: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(len).ok_or(DecodeError::Truncated)?;
        let bytes = self.buf.get(self.pos..end).ok_or(DecodeError::Truncated)?;
        self.pos = end;
        Ok(bytes)
    }

    fn read_u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, DecodeError> {
        let bytes = self.take(2)?;
        Ok(u16::from_ne_bytes(
            bytes.try_into().expect("take(2) returns two bytes"),
        ))
    }

    fn read_u32(&mut self) -> Result<u32, DecodeError> {
        let bytes = self.take(4)?;
        Ok(u32::from_ne_bytes(
            bytes.try_into().expect("take(4) returns four bytes"),
        ))
    }

    fn read_i32(&mut self) -> Result<i32, DecodeError> {
        let bytes = self.take(4)?;
        Ok(i32::from_ne_bytes(
            bytes.try_into().expect("take(4) returns four bytes"),
        ))
    }

    fn read_i64(&mut self) -> Result<i64, DecodeError> {
        let bytes = self.take(8)?;
        Ok(i64::from_ne_bytes(
            bytes.try_into().expect("take(8) returns eight bytes"),
        ))
    }

    fn read_f64(&mut self) -> Result<f64, DecodeError> {
        let bytes = self.take(8)?;
        Ok(f64::from_bits(u64::from_ne_bytes(
            bytes.try_into().expect("take(8) returns eight bytes"),
        )))
    }

    fn read_size(&mut self) -> Result<usize, DecodeError> {
        match self.read_u8()? {
            SIZE_U16 => Ok(usize::from(self.read_u16()?)),
            SIZE_U32 => {
                let n = self.read_u32()?;
                // Infallible on every 32- and 64-bit target; a 16-bit one could
                // not hold the payload anyway.
                usize::try_from(n).map_err(|_| DecodeError::Truncated)
            }
            n => Ok(usize::from(n)),
        }
    }

    fn read_string(&mut self, len: usize) -> Result<String, DecodeError> {
        let bytes = self.take(len)?;
        // Dart writes UTF-8, so this only replaces characters for a corrupted
        // or mis-codec'd message.
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    fn read_u8_list(&mut self, len: usize) -> Result<Vec<u8>, DecodeError> {
        Ok(self.take(len)?.to_vec())
    }

    fn read_i32_list(&mut self, len: usize) -> Result<Vec<i32>, DecodeError> {
        self.align_to(4);
        Ok(self
            .take_elements(len, 4)?
            .map(|bytes| i32::from_ne_bytes(bytes.try_into().expect("four-byte chunk")))
            .collect())
    }

    fn read_i64_list(&mut self, len: usize) -> Result<Vec<i64>, DecodeError> {
        self.align_to(8);
        Ok(self
            .take_elements(len, 8)?
            .map(|bytes| i64::from_ne_bytes(bytes.try_into().expect("eight-byte chunk")))
            .collect())
    }

    fn read_f32_list(&mut self, len: usize) -> Result<Vec<f32>, DecodeError> {
        self.align_to(4);
        Ok(self
            .take_elements(len, 4)?
            .map(|bytes| {
                f32::from_bits(u32::from_ne_bytes(
                    bytes.try_into().expect("four-byte chunk"),
                ))
            })
            .collect())
    }

    fn read_f64_list(&mut self, len: usize) -> Result<Vec<f64>, DecodeError> {
        self.align_to(8);
        Ok(self
            .take_elements(len, 8)?
            .map(|bytes| {
                f64::from_bits(u64::from_ne_bytes(
                    bytes.try_into().expect("eight-byte chunk"),
                ))
            })
            .collect())
    }

    /// `len` chunks of `element_size` bytes, taken in one bounds-checked read.
    ///
    /// Claiming the whole payload up front is what keeps a bogus wire length
    /// from reaching `Vec::with_capacity`: the returned iterator is
    /// `ExactSizeIterator`, so `collect` sizes the vector from bytes that are
    /// really there rather than from the number the message claimed.
    fn take_elements(
        &mut self,
        len: usize,
        element_size: usize,
    ) -> Result<impl ExactSizeIterator<Item = &'a [u8]>, DecodeError> {
        let size = len
            .checked_mul(element_size)
            .ok_or(DecodeError::Truncated)?;
        Ok(self.take(size)?.chunks_exact(element_size))
    }

    /// Skip padding so that the next read starts on an `align`-byte boundary,
    /// counted from the start of the message.
    fn align_to(&mut self, align: usize) {
        let m = self.pos % align;
        if m > 0 {
            // Saturating: an overlong skip simply leaves the cursor past the
            // end of the buffer, where the next `take` reports a truncation.
            self.pos = self.pos.saturating_add(align - m);
        }
    }
}

/// The encode-side counterpart of [`Reader`].
struct Writer(Vec<u8>);

impl Writer {
    fn with_capacity(capacity: usize) -> Self {
        Writer(Vec::with_capacity(capacity))
    }
    fn write_u8(&mut self, n: u8) {
        self.0.push(n);
    }
    fn write_bytes(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
    fn write_u16(&mut self, n: u16) {
        self.write_bytes(&n.to_ne_bytes());
    }
    fn write_u32(&mut self, n: u32) {
        self.write_bytes(&n.to_ne_bytes());
    }
    fn write_i32(&mut self, n: i32) {
        self.write_bytes(&n.to_ne_bytes());
    }
    fn write_i64(&mut self, n: i64) {
        self.write_bytes(&n.to_ne_bytes());
    }
    fn write_f64(&mut self, n: f64) {
        self.write_bytes(&n.to_bits().to_ne_bytes());
    }

    /// Write a size prefix.
    ///
    /// # Panics
    ///
    /// Panics for a size above `u32::MAX`, which the format cannot express.
    fn write_size(&mut self, n: usize) {
        if n < usize::from(SIZE_U16) {
            self.write_u8(u8::try_from(n).expect("n < 254"));
        } else if let Ok(n) = u16::try_from(n) {
            self.write_u8(SIZE_U16);
            self.write_u16(n);
        } else if let Ok(n) = u32::try_from(n) {
            self.write_u8(SIZE_U32);
            self.write_u32(n);
        } else {
            panic!("value does not fit in the standard codec size encoding: {n}");
        }
    }

    /// Pad so that the next write starts on an `align`-byte boundary, counted
    /// from the start of the message.
    fn align_to(&mut self, align: usize) {
        let m = self.0.len() % align;
        if m == 0 {
            return;
        }
        self.0.resize(self.0.len() + (align - m), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every fixture in this module came out of the Dart SDK's own
    /// `StandardMessageCodec` / `StandardMethodCodec` (3.12.2, x86-64), so the
    /// tests pin the real wire format rather than this implementation's idea of
    /// it. Dart writes `Endian.host`, which is why the byte order below is the
    /// host's and `from_ne_bytes` is correct.
    fn round_trip(value: &Value, dart_bytes: &[u8]) {
        let encoded = STANDARD_CODEC.encode_message(value);
        assert_eq!(encoded, dart_bytes, "encoding {value:?}");
        let decoded = STANDARD_CODEC
            .decode_message(dart_bytes)
            .expect("the Dart fixture decodes");
        assert_eq!(&decoded, value, "decoding {dart_bytes:?}");
    }

    #[test]
    fn scalars_match_the_dart_encoding() {
        round_trip(&Value::Null, &[0]);
        round_trip(&Value::Boolean(true), &[1]);
        round_trip(&Value::Boolean(false), &[2]);
        round_trip(&Value::I32(7), &[3, 7, 0, 0, 0]);
        round_trip(&Value::I32(-2), &[3, 254, 255, 255, 255]);
        round_trip(&Value::I64(1 << 40), &[4, 0, 0, 0, 0, 0, 1, 0, 0]);
        round_trip(
            &Value::F64(1.5),
            &[6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 248, 63],
        );
    }

    #[test]
    fn strings_match_the_dart_encoding() {
        round_trip(&Value::String(String::new()), &[7, 0]);
        round_trip(
            &Value::String("hello".into()),
            &[7, 5, 104, 101, 108, 108, 111],
        );
        // Multi-byte characters are counted in bytes, not characters.
        round_trip(
            &Value::String("héllo".into()),
            &[7, 6, 104, 195, 169, 108, 108, 111],
        );
    }

    /// Typed lists are padded relative to the start of the *message*, which is
    /// why the nested-map fixture below has a single pad byte rather than seven.
    #[test]
    fn typed_lists_match_the_dart_alignment() {
        round_trip(&Value::U8List(vec![1, 2, 3]), &[8, 3, 1, 2, 3]);
        round_trip(
            &Value::I32List(vec![1, -2]),
            &[9, 2, 0, 0, 1, 0, 0, 0, 254, 255, 255, 255],
        );
        round_trip(
            &Value::I64List(vec![1]),
            &[10, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0],
        );
        round_trip(&Value::F32List(vec![1.5]), &[14, 1, 0, 0, 0, 0, 192, 63]);
        round_trip(
            &Value::F64List(vec![1.5, 2.5]),
            &[
                11, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 248, 63, 0, 0, 0, 0, 0, 0, 4, 64,
            ],
        );
    }

    #[test]
    fn containers_match_the_dart_encoding() {
        round_trip(&Value::List(vec![]), &[12, 0]);
        round_trip(
            &Value::List(vec![Value::I32(1), Value::String("a".into()), Value::Null]),
            &[12, 3, 3, 1, 0, 0, 0, 7, 1, 97, 0],
        );
        round_trip(&Value::Map(HashMap::new()), &[13, 0]);
        round_trip(
            &Value::Map(HashMap::from([("a".to_owned(), Value::I32(1))])),
            &[13, 1, 7, 1, 97, 3, 1, 0, 0, 0],
        );
        round_trip(
            &Value::Map(HashMap::from([("k".to_owned(), Value::F64List(vec![1.5]))])),
            &[13, 1, 7, 1, 107, 11, 1, 0, 0, 0, 0, 0, 0, 0, 248, 63],
        );
    }

    /// A string whose length prefix runs past the end of the buffer used to
    /// build a slice with `slice::from_raw_parts` and hand ~199 bytes of
    /// adjacent heap memory back to Dart.
    #[test]
    fn a_truncated_string_is_rejected_instead_of_read_out_of_bounds() {
        assert!(matches!(
            STANDARD_CODEC.decode_message(&[VALUE_STRING, 200, b'A']),
            Err(DecodeError::Truncated)
        ));
        assert!(matches!(
            STANDARD_CODEC.decode_message(&[VALUE_STRING, 5, b'A']),
            Err(DecodeError::Truncated)
        ));
    }

    /// A four-gigabyte length prefix in a seven-byte message must not reach
    /// `Vec::with_capacity`.
    #[test]
    fn a_huge_length_prefix_is_rejected_without_allocating() {
        for tag in [
            VALUE_STRING,
            VALUE_UINT8LIST,
            VALUE_INT32LIST,
            VALUE_INT64LIST,
            VALUE_FLOAT32LIST,
            VALUE_FLOAT64LIST,
            VALUE_LIST,
            VALUE_MAP,
        ] {
            let buf = [tag, SIZE_U32, 255, 255, 255, 255];
            assert!(
                STANDARD_CODEC.decode_message(&buf).is_err(),
                "tag {tag} with a u32::MAX length"
            );
        }
    }

    #[test]
    fn an_empty_or_truncated_message_is_rejected() {
        assert!(matches!(
            STANDARD_CODEC.decode_message(&[]),
            Err(DecodeError::Truncated)
        ));
        // Tag present, payload missing.
        for buf in [&[VALUE_INT32][..], &[VALUE_INT64], &[VALUE_FLOAT64], &[7]] {
            assert!(matches!(
                STANDARD_CODEC.decode_message(buf),
                Err(DecodeError::Truncated)
            ));
        }
    }

    /// Returning `Value::Null` for an unknown tag left the size prefix and
    /// payload unread, so every later value in the same message decoded as
    /// garbage.
    #[test]
    fn an_unknown_type_tag_is_rejected() {
        assert!(matches!(
            STANDARD_CODEC.decode_message(&[200, 0]),
            Err(DecodeError::UnknownType(200))
        ));
        // ... including inside a list, where the old behaviour was worst.
        assert!(matches!(
            STANDARD_CODEC.decode_message(&[VALUE_LIST, 2, 200, 0, 3, 1, 0, 0, 0]),
            Err(DecodeError::UnknownType(200))
        ));
    }

    /// Tag 14 is `Float32List`, which Flutter really does write; it used to
    /// decode as `Value::Null` with the payload left on the wire.
    #[test]
    fn a_float32_list_decodes_as_f32list() {
        assert_eq!(
            STANDARD_CODEC
                .decode_message(&[14, 1, 0, 0, 0, 0, 192, 63])
                .expect("a Dart Float32List decodes"),
            Value::F32List(vec![1.5])
        );
    }

    /// Tag 5 is a big integer, written as a length-prefixed hexadecimal string.
    /// It used to be `panic!("Not implemented")`, which aborts the process from
    /// inside the engine's callback.
    #[test]
    fn a_large_int_decodes_as_a_string_instead_of_panicking() {
        assert_eq!(
            STANDARD_CODEC
                .decode_message(&[VALUE_LARGEINT, 2, b'f', b'f'])
                .expect("a large int decodes"),
            Value::String("ff".into())
        );
    }

    #[test]
    fn a_map_with_a_non_string_key_is_rejected() {
        assert!(matches!(
            STANDARD_CODEC.decode_message(&[VALUE_MAP, 1, 3, 1, 0, 0, 0, 0]),
            Err(DecodeError::WrongType { .. })
        ));
    }

    /// 253 is the last single-byte size, 254 introduces a `u16` and 255 a
    /// `u32`; `u32::MAX` is a valid size that the old `n < u32::MAX` guard
    /// rejected with a panic.
    #[test]
    fn size_prefixes_round_trip_at_every_boundary() {
        for n in [0_usize, 1, 253, 254, 0xffff, 0x1_0000, 0xffff_ffff] {
            let mut writer = Writer::with_capacity(8);
            writer.write_size(n);
            let encoded = writer.0;
            assert_eq!(
                Reader::new(&encoded).read_size().expect("a size decodes"),
                n,
                "size {n} encoded as {encoded:?}"
            );
        }
        // Dart's own encoding of a 253- and a 254-byte string, for the boundary
        // between the one-byte and the `u16` form.
        assert_eq!(
            STANDARD_CODEC.encode_message(&Value::String("a".repeat(253)))[..2],
            [7, 253]
        );
        assert_eq!(
            STANDARD_CODEC.encode_message(&Value::String("a".repeat(254)))[..4],
            [7, 254, 254, 0]
        );
    }

    #[test]
    fn method_calls_match_the_dart_encoding() {
        let call = MethodCall {
            method: "foo".into(),
            args: Value::Null,
        };
        assert_eq!(
            STANDARD_CODEC.encode_method_call(&call),
            [7, 3, 102, 111, 111, 0]
        );

        let decoded = STANDARD_CODEC
            .decode_method_call(&[7, 3, 102, 111, 111, 3, 7, 0, 0, 0])
            .expect("a Dart method call decodes");
        assert_eq!(decoded.method, "foo");
        assert_eq!(decoded.args, Value::I32(7));
    }

    #[test]
    fn a_method_call_without_a_string_name_is_rejected() {
        assert!(matches!(
            STANDARD_CODEC.decode_method_call(&[3, 1, 0, 0, 0, 0]),
            Err(DecodeError::WrongType { .. })
        ));
        assert!(matches!(
            STANDARD_CODEC.decode_method_call(&[]),
            Err(DecodeError::Truncated)
        ));
    }

    #[test]
    fn envelopes_match_the_dart_encoding() {
        assert_eq!(STANDARD_CODEC.encode_success_envelope(&Value::Null), [0, 0]);
        assert!(matches!(
            STANDARD_CODEC
                .decode_envelope(&[0, 3, 42, 0, 0, 0])
                .expect("a Dart success envelope decodes"),
            MethodCallResult::Ok(Value::I32(42))
        ));

        assert_eq!(
            STANDARD_CODEC.encode_error_envelope("E", "m", &Value::Null),
            [1, 7, 1, 69, 7, 1, 109, 0]
        );
        let error = STANDARD_CODEC
            .decode_envelope(&[1, 7, 1, 69, 7, 1, 109, 0])
            .expect("a Dart error envelope decodes");
        match error {
            MethodCallResult::Err {
                code,
                message,
                details,
            } => {
                assert_eq!(code, "E");
                assert_eq!(message, "m");
                assert_eq!(details, Value::Null);
            }
            other => panic!("expected an error envelope, got {other:?}"),
        }
    }

    /// The framework answers an unhandled call with an empty envelope, which
    /// used to index an empty buffer and panic.
    #[test]
    fn an_empty_envelope_is_not_implemented() {
        assert!(matches!(
            STANDARD_CODEC
                .decode_envelope(&[])
                .expect("an empty envelope decodes"),
            MethodCallResult::NotImplemented
        ));
        assert!(
            STANDARD_CODEC
                .encode_method_call_response(&MethodCallResult::NotImplemented)
                .is_empty()
        );
    }

    #[test]
    fn an_envelope_with_an_unknown_discriminator_is_rejected() {
        assert!(matches!(
            STANDARD_CODEC.decode_envelope(&[2, 0]),
            Err(DecodeError::WrongType { .. })
        ));
    }

    /// Dart sends UTF-8, but a corrupted or mis-codec'd message must still come
    /// back as a value rather than as undefined behaviour.
    #[test]
    fn invalid_utf8_in_a_string_is_replaced_not_rejected() {
        assert_eq!(
            STANDARD_CODEC
                .decode_message(&[VALUE_STRING, 2, 0xff, 0xfe])
                .expect("invalid UTF-8 decodes lossily"),
            Value::String("\u{fffd}\u{fffd}".into())
        );
    }
}
