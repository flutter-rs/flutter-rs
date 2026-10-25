//! Plugin for the `flutter/keyboard` channel.
//!
//! `HardwareKeyboard.syncKeyboardState()` calls `getKeyboardState` exactly
//! once, from `ServicesBinding.initInstances`, to learn which keys were already
//! held when the app started, and nothing re-queries the channel later. The
//! answer is a `Map<int, int>` from physical to logical key id — the same ids
//! the embedder puts in its `FlutterKeyEvent`s — which an embedder can only
//! produce by keeping its own table of held keys. Hand one in through
//! [`KeyboardPlugin::new`]; [`KeyboardPlugin::default`] answers "no
//! information", which the framework accepts but which also means the state is
//! never resynchronised.
//!
//! An embedder that can lose keyboard focus mid-press must still synthesise the
//! missing `KeyEventType::Up` itself: this channel is not a fallback for a key
//! the framework thinks is held forever.

use std::sync::{Arc, Weak};

use log::warn;
use parking_lot::Mutex;

use flutter_engine::{
    FlutterEngine,
    channel::{MethodCall, MethodCallHandler, MethodChannel},
    codec::{self, DecodeError, MethodCallResult, MethodCodec, STANDARD_CODEC, Value},
    plugins::Plugin,
};

/// The name this plugin is registered under.
pub const PLUGIN_NAME: &str = module_path!();
/// The channel this plugin handles.
pub const CHANNEL_NAME: &str = "flutter/keyboard";

/// The embedder half of `flutter/keyboard`: which keys are held right now.
///
/// GLFW, Win32 and the X11/Wayland protocols all lack a synchronous "what is
/// down?" query, so the answer has to be a table the embedder maintains from
/// the key events it already translates — including clearing it when the window
/// loses keyboard focus, after synthesising the key-ups the framework is still
/// waiting for.
pub trait KeyboardStateHandler {
    /// Every key currently held, as `(physical key id, logical key id)` pairs.
    ///
    /// The ids must be the ones the embedder sends in its `FlutterKeyEvent`s —
    /// the framework uses them to key its own `_pressedKeys` map, so a
    /// mismatch leaves a phantom key held.
    fn pressed_keys(&mut self) -> Vec<(u64, u64)>;
}

/// Plugin for the `flutter/keyboard` channel.
#[derive(Default)]
pub struct KeyboardPlugin {
    channel: Weak<MethodChannel>,
    state: Option<Arc<Mutex<dyn KeyboardStateHandler + Send>>>,
}

impl KeyboardPlugin {
    /// A plugin that answers `getKeyboardState` from `state`.
    #[must_use]
    pub fn new(state: Arc<Mutex<dyn KeyboardStateHandler + Send>>) -> Self {
        Self {
            channel: Weak::new(),
            state: Some(state),
        }
    }
}

impl Plugin for KeyboardPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init(&mut self, engine: &FlutterEngine) {
        self.channel = engine.register_channel(MethodChannel::new(
            CHANNEL_NAME,
            Handler {
                state: self.state.clone(),
            },
            &KEYBOARD_STATE_CODEC,
        ));
    }
}

struct Handler {
    state: Option<Arc<Mutex<dyn KeyboardStateHandler + Send>>>,
}

impl MethodCallHandler for Handler {
    fn on_method_call(&mut self, call: MethodCall) {
        match call.method() {
            "getKeyboardState" => {
                let Some(state) = &self.state else {
                    // `HardwareKeyboard.syncKeyboardState` reads null as "no
                    // information" and leaves its own table alone, which is
                    // the honest answer from an embedder that does not track
                    // the keyboard.
                    call.success(Value::Null);
                    return;
                };
                let pressed = state.lock().pressed_keys();
                call.success(Value::List(keyboard_state_reply(&pressed)));
            }
            _ => call.not_implemented(),
        }
    }
}

/// The flat `[physical, logical, …]` list [`KeyboardStateCodec`] turns into the
/// `Map<int, int>` the framework reads.
///
/// A key id that does not fit in an `i64` cannot be put on the wire — the
/// standard codec's integers are signed — and dropping that one entry is better
/// than failing the whole reply, so it is logged and skipped. Flutter's own
/// physical and logical ids are far below the limit; only a hand-written
/// embedder mapping can get here.
fn keyboard_state_reply(pressed: &[(u64, u64)]) -> Vec<Value> {
    let mut reply = Vec::with_capacity(pressed.len() * 2);
    for &(physical, logical) in pressed {
        match (i64::try_from(physical), i64::try_from(logical)) {
            (Ok(physical), Ok(logical)) => {
                reply.push(Value::I64(physical));
                reply.push(Value::I64(logical));
            }
            _ => warn!(
                "dropping held key {physical:#x} -> {logical:#x} from the keyboard state: \
                 the ids do not fit in the codec's signed integers"
            ),
        }
    }
    reply
}

/// The `flutter/keyboard` channel's codec.
///
/// Byte for byte Dart's `StandardMethodCodec`, except for the one thing this
/// channel needs and [`Value`] cannot express: `getKeyboardState` is read with
/// `invokeMapMethod<int, int>`, so its reply has to be a Dart map keyed by
/// *integers*, while [`Value::Map`] is keyed by `String` and `Map.cast<int,
/// int>` throws on a string key. Routing the reply through a [`Value`] is
/// unavoidable, because that is what [`MethodCall::success`] encodes, so this
/// codec reads the only successful reply the channel ever makes — the flat
/// `[physical, logical, …]` list [`keyboard_state_reply`] builds — as that map.
/// Anything else is handed to the standard codec untouched.
struct KeyboardStateCodec;

const KEYBOARD_STATE_CODEC: KeyboardStateCodec = KeyboardStateCodec;

impl MethodCodec for KeyboardStateCodec {
    fn decode_method_call(&self, buf: &[u8]) -> Result<codec::MethodCall, DecodeError> {
        STANDARD_CODEC.decode_method_call(buf)
    }

    fn encode_success_envelope(&self, v: &Value) -> Vec<u8> {
        match int_pairs(v) {
            Some(entries) => encode_int_map_envelope(&entries),
            None => STANDARD_CODEC.encode_success_envelope(v),
        }
    }

    fn encode_error_envelope(&self, code: &str, message: &str, details: &Value) -> Vec<u8> {
        STANDARD_CODEC.encode_error_envelope(code, message, details)
    }

    fn encode_method_call(&self, v: &codec::MethodCall) -> Vec<u8> {
        STANDARD_CODEC.encode_method_call(v)
    }

    fn decode_envelope(&self, buf: &[u8]) -> Result<MethodCallResult, DecodeError> {
        STANDARD_CODEC.decode_envelope(buf)
    }
}

/// Read `v` as consecutive `(key, value)` integer pairs, or `None` if it is not
/// an even-length sequence of integers.
///
/// All three integer sequence shapes are accepted because `to_value` decides
/// which one a `Vec` of integers becomes, and that is not this module's
/// business.
fn int_pairs(v: &Value) -> Option<Vec<(i64, i64)>> {
    let ints: Vec<i64> = match v {
        Value::List(items) => items.iter().map(as_int).collect::<Option<_>>()?,
        Value::I32List(items) => items.iter().copied().map(i64::from).collect(),
        Value::I64List(items) => items.clone(),
        _ => return None,
    };
    // An odd length is not a map, and a length the size prefix cannot express
    // is not encodable at all; both fall back to the standard codec.
    if !ints.len().is_multiple_of(2) || u32::try_from(ints.len() / 2).is_err() {
        return None;
    }
    Some(
        ints.as_chunks::<2>()
            .0
            .iter()
            .map(|&[key, value]| (key, value))
            .collect(),
    )
}

fn as_int(v: &Value) -> Option<i64> {
    match v {
        Value::I32(n) => Some(i64::from(*n)),
        Value::I64(n) => Some(*n),
        _ => None,
    }
}

/// Dart's `StandardMethodCodec` success envelope around a `Map<int, int>`.
///
/// Mirrors `StandardMessageCodec.writeValue`: a `0` envelope byte, the map tag,
/// the entry count as a size prefix, then every key and value as a tagged
/// integer. Integers carry no alignment padding — only `Float64` and the typed
/// lists are aligned — and multi-byte payloads go out in the host's byte order,
/// which is what Dart's `WriteBuffer` writes and `ReadBuffer` reads.
fn encode_int_map_envelope(entries: &[(i64, i64)]) -> Vec<u8> {
    /// The standard envelope's "success" discriminator.
    const ENVELOPE_SUCCESS: u8 = 0;
    const VALUE_INT32: u8 = 3;
    const VALUE_INT64: u8 = 4;
    const VALUE_MAP: u8 = 13;

    // Envelope, tag and size prefix, then at most nine bytes per integer.
    let mut buf = Vec::with_capacity(7 + entries.len() * 18);
    buf.push(ENVELOPE_SUCCESS);
    buf.push(VALUE_MAP);
    write_size(&mut buf, entries.len());
    for &(key, value) in entries {
        for n in [key, value] {
            // Dart reads either tag as an `int`, so prefer the narrow one;
            // many of Flutter's logical key ids need the wide one.
            if let Ok(n) = i32::try_from(n) {
                buf.push(VALUE_INT32);
                buf.extend_from_slice(&n.to_ne_bytes());
            } else {
                buf.push(VALUE_INT64);
                buf.extend_from_slice(&n.to_ne_bytes());
            }
        }
    }
    buf
}

/// Write the standard codec's size prefix: one byte below 254, otherwise 254
/// and a `u16`, or 255 and a `u32`.
///
/// # Panics
///
/// Panics for a size above `u32::MAX`, which the format cannot express.
/// [`int_pairs`] rejects such a reply before it gets here.
fn write_size(buf: &mut Vec<u8>, n: usize) {
    /// The size prefix that introduces a `u16` length.
    const SIZE_U16: u8 = 254;
    /// The size prefix that introduces a `u32` length.
    const SIZE_U32: u8 = 255;

    if n < usize::from(SIZE_U16) {
        buf.push(u8::try_from(n).expect("n < 254"));
    } else if let Ok(n) = u16::try_from(n) {
        buf.push(SIZE_U16);
        buf.extend_from_slice(&n.to_ne_bytes());
    } else {
        buf.push(SIZE_U32);
        buf.extend_from_slice(
            &u32::try_from(n)
                .expect("int_pairs rejects a longer reply")
                .to_ne_bytes(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `(physical, logical)` pair for the left Shift key, which has a
    /// logical id that does not fit in 32 bits.
    const LEFT_SHIFT: (u64, u64) = (0x0007_00e1, 0x0002_0000_0102);
    /// The `(physical, logical)` pair for `A`, both of which do fit.
    const KEY_A: (u64, u64) = (0x0007_0004, 0x0000_0061);

    #[test]
    fn a_reply_is_the_held_keys_flattened_into_physical_logical_pairs() {
        assert_eq!(keyboard_state_reply(&[]), Vec::<Value>::new());
        assert_eq!(
            keyboard_state_reply(&[KEY_A, LEFT_SHIFT]),
            vec![
                Value::I64(0x0007_0004),
                Value::I64(0x0000_0061),
                Value::I64(0x0007_00e1),
                Value::I64(0x0002_0000_0102),
            ]
        );
    }

    /// Dropping one unencodable entry keeps the rest of the state; failing the
    /// whole reply would leave the framework with no information at all.
    #[test]
    fn a_key_id_too_large_for_the_wire_is_skipped_not_fatal() {
        assert_eq!(
            keyboard_state_reply(&[KEY_A, (u64::MAX, 1), (1, u64::MAX)]),
            vec![Value::I64(0x0007_0004), Value::I64(0x0000_0061)]
        );
    }

    /// Dart's `StandardMessageCodec` writes a map as the tag, the entry count
    /// and then each key and value as a tagged, unaligned, host-endian
    /// integer. These fixtures are that format by hand; `ReadBuffer` reads
    /// `Endian.host`, which is why `to_ne_bytes` is correct.
    #[test]
    fn an_empty_state_encodes_as_darts_empty_map() {
        // 0x00 success, 0x0d map, 0x00 entries.
        assert_eq!(
            KEYBOARD_STATE_CODEC.encode_success_envelope(&Value::List(Vec::new())),
            vec![0x00, 0x0d, 0x00]
        );
    }

    #[test]
    fn a_held_key_encodes_as_an_int_keyed_map_entry() {
        let reply = Value::List(keyboard_state_reply(&[KEY_A]));
        let mut expected = vec![0x00, 0x0d, 0x01];
        expected.push(0x03); // int32 physical
        expected.extend_from_slice(&0x0007_0004_i32.to_ne_bytes());
        expected.push(0x03); // int32 logical
        expected.extend_from_slice(&0x0000_0061_i32.to_ne_bytes());

        assert_eq!(
            KEYBOARD_STATE_CODEC.encode_success_envelope(&reply),
            expected
        );
    }

    /// Logical ids for modifiers and non-printable keys live above
    /// `0x1_0000_0000`, so the encoder has to widen to the int64 tag or the
    /// value arrives truncated.
    #[test]
    fn a_logical_id_above_32_bits_uses_the_int64_tag() {
        let reply = Value::List(keyboard_state_reply(&[LEFT_SHIFT]));
        let mut expected = vec![0x00, 0x0d, 0x01];
        expected.push(0x03); // int32 physical
        expected.extend_from_slice(&0x0007_00e1_i32.to_ne_bytes());
        expected.push(0x04); // int64 logical
        expected.extend_from_slice(&0x0002_0000_0102_i64.to_ne_bytes());

        assert_eq!(
            KEYBOARD_STATE_CODEC.encode_success_envelope(&reply),
            expected
        );
    }

    /// The entry count is a size prefix, so it changes shape at 254 — a
    /// boundary no keyboard reaches but the format still defines.
    #[test]
    fn a_long_state_uses_the_u16_size_prefix() {
        let pressed: Vec<(u64, u64)> = (0..254).map(|i| (i, i)).collect();
        let reply = Value::List(keyboard_state_reply(&pressed));
        let encoded = KEYBOARD_STATE_CODEC.encode_success_envelope(&reply);

        assert_eq!(encoded[0], 0x00, "success envelope");
        assert_eq!(encoded[1], 0x0d, "map tag");
        assert_eq!(encoded[2], 254, "u16 size prefix");
        assert_eq!(encoded[3..5], 254_u16.to_ne_bytes());
        // 254 entries of two int32s, each a tag and four bytes.
        assert_eq!(encoded.len(), 5 + 254 * 2 * 5);
    }

    /// Everything except the keyboard state goes to the standard codec, so an
    /// error reply and a method call keep their normal encoding.
    #[test]
    fn non_state_values_are_encoded_by_the_standard_codec() {
        for value in [
            Value::Null,
            Value::String("nope".to_owned()),
            // An odd number of integers is not a map.
            Value::List(vec![Value::I64(1), Value::I64(2), Value::I64(3)]),
            // Nor is a list with a non-integer in it.
            Value::List(vec![Value::I64(1), Value::Boolean(true)]),
        ] {
            assert_eq!(
                KEYBOARD_STATE_CODEC.encode_success_envelope(&value),
                STANDARD_CODEC.encode_success_envelope(&value),
                "{value:?}"
            );
        }

        assert_eq!(
            KEYBOARD_STATE_CODEC.encode_error_envelope("code", "message", &Value::Null),
            STANDARD_CODEC.encode_error_envelope("code", "message", &Value::Null)
        );
    }

    /// `to_value` is free to narrow a sequence of integers to a typed list, so
    /// the codec must read the state out of any of the three shapes.
    #[test]
    fn every_integer_sequence_shape_is_read_as_a_map() {
        let expected = KEYBOARD_STATE_CODEC
            .encode_success_envelope(&Value::List(vec![Value::I32(7), Value::I32(9)]));

        assert_eq!(
            KEYBOARD_STATE_CODEC.encode_success_envelope(&Value::I32List(vec![7, 9])),
            expected
        );
        assert_eq!(
            KEYBOARD_STATE_CODEC.encode_success_envelope(&Value::I64List(vec![7, 9])),
            expected
        );
        assert_eq!(
            KEYBOARD_STATE_CODEC
                .encode_success_envelope(&Value::List(vec![Value::I64(7), Value::I32(9)])),
            expected
        );
    }

    /// Only `encode_success_envelope` is special; a method call Dart sends and
    /// the envelope it replies with still round-trip through the standard
    /// codec.
    #[test]
    fn method_calls_round_trip_through_the_standard_codec() {
        let call = codec::MethodCall {
            method: "getKeyboardState".to_owned(),
            args: Value::Null,
        };
        let encoded = KEYBOARD_STATE_CODEC.encode_method_call(&call);
        let decoded = KEYBOARD_STATE_CODEC
            .decode_method_call(&encoded)
            .expect("the standard codec encoded it");

        assert_eq!(decoded.method, "getKeyboardState");
        assert_eq!(decoded.args, Value::Null);
    }
}
