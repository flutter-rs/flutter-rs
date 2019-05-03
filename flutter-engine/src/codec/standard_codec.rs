use super::{MethodCall, MethodCallResult, MethodCodec, Value};

use std::{collections::HashMap, mem, slice, u16, u32};

use log::error;

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

#[derive(Debug)]
enum DecodeError {
    Invalid,
    Ended,
}

pub struct StandardMethodCodec;

pub const CODEC: StandardMethodCodec = StandardMethodCodec {};

impl StandardMethodCodec {
    fn read_value(reader: &mut Reader) -> Result<Value, DecodeError> {
        if reader.ended() {
            return Err(DecodeError::Ended);
        }

        let t = reader.read_u8();
        Ok(match t {
            VALUE_NULL => Value::Null,
            VALUE_FALSE => Value::Boolean(false),
            VALUE_TRUE => Value::Boolean(true),
            VALUE_INT32 => Value::I32(reader.read_i32()),
            VALUE_INT64 => Value::I64(reader.read_i64()),
            VALUE_LARGEINT => panic!("Not implemented"),
            VALUE_FLOAT64 => Value::F64(reader.read_f64()),
            VALUE_STRING => {
                let len = reader.read_size();
                Value::String(reader.read_string(len))
            }
            VALUE_UINT8LIST => {
                let len = reader.read_size();
                Value::U8List(reader.read_u8_list(len))
            }
            VALUE_INT32LIST => {
                let len = reader.read_size();
                Value::I32List(reader.read_i32_list(len))
            }
            VALUE_INT64LIST => {
                let len = reader.read_size();
                Value::I64List(reader.read_i64_list(len))
            }
            VALUE_FLOAT64LIST => {
                let len = reader.read_size();
                Value::F64List(reader.read_f64_list(len))
            }
            VALUE_LIST => {
                let len = reader.read_size();
                let mut list = Vec::new();
                for _ in 0..len {
                    if let Ok(e) = Self::read_value(reader) {
                        list.push(e);
                    } else {
                        return Err(DecodeError::Invalid);
                    }
                }
                Value::List(list)
            }
            VALUE_MAP => {
                let len = reader.read_size();
                let mut map = HashMap::new();
                for _ in 0..len {
                    let k = Self::read_value(reader);
                    let v = Self::read_value(reader);
                    if k.is_err() || v.is_err() {
                        return Err(DecodeError::Invalid);
                    }
                    let k = k.unwrap();
                    let v = v.unwrap();
                    if let Value::String(k) = k {
                        map.insert(k, v);
                    } else {
                        return Err(DecodeError::Invalid);
                    }
                }
                Value::Map(map)
            }
            _ => Value::Null,
        })
    }
    fn write_string(writer: &mut Writer, s: &String) {
        writer.write_u8(VALUE_STRING);
        writer.write_size(s.len());
        writer.write_string(s);
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
            Value::String(s) => {
                Self::write_string(writer, s);
            }
            Value::U8List(list) => {
                writer.write_u8(VALUE_UINT8LIST);
                writer.align_to(8);
                for n in list {
                    writer.write_u8(*n);
                }
            }
            Value::I32List(list) => {
                writer.write_u8(VALUE_INT32LIST);
                writer.align_to(8);
                for n in list {
                    writer.write_i32(*n);
                }
            }
            Value::I64List(list) => {
                writer.write_u8(VALUE_INT64LIST);
                writer.align_to(8);
                for n in list {
                    writer.write_i64(*n);
                }
            }
            Value::F64List(list) => {
                writer.write_u8(VALUE_FLOAT64LIST);
                writer.align_to(8);
                for n in list {
                    writer.write_f64(*n);
                }
            }
            Value::List(list) => {
                writer.write_u8(VALUE_LIST);
                writer.write_size(list.len());
                list.iter().for_each(|v| {
                    Self::write_value(writer, v);
                });
            }
            Value::Map(map) => {
                writer.write_u8(VALUE_MAP);
                writer.write_size(map.len());
                map.iter().for_each(|(k, v)| {
                    Self::write_string(writer, k);
                    Self::write_value(writer, v);
                });
            }
            _ => (),
        }
    }
}

impl MethodCodec for StandardMethodCodec {
    fn encode_method_call(&self, v: &MethodCall) -> Vec<u8> {
        let mut writer = Writer::new(Vec::new());
        // Can we avoid this clone?
        StandardMethodCodec::write_value(&mut writer, &Value::String(v.method.to_owned()));
        StandardMethodCodec::write_value(&mut writer, &v.args);
        writer.0
    }

    fn decode_method_call(&self, buf: &[u8]) -> Option<MethodCall> {
        let mut reader = Reader::new(buf);
        let method: Value = StandardMethodCodec::read_value(&mut reader).unwrap();
        let args: Value = StandardMethodCodec::read_value(&mut reader).unwrap();

        if let Value::String(method) = method {
            return Some(MethodCall { method, args });
        }
        error!("Invalid method call");
        None
    }

    fn encode_success_envelope(&self, result: &Value) -> Vec<u8> {
        let mut writer = Writer::new(Vec::new());
        writer.write_u8(0);
        StandardMethodCodec::write_value(&mut writer, result);
        writer.0
    }

    fn encode_error_envelope(&self, code: &str, message: &str, v: &Value) -> Vec<u8> {
        let mut writer = Writer::new(Vec::new());
        writer.write_u8(1);
        StandardMethodCodec::write_value(&mut writer, &Value::String(code.to_owned()));
        StandardMethodCodec::write_value(&mut writer, &Value::String(message.to_owned()));
        StandardMethodCodec::write_value(&mut writer, v);
        writer.0
    }

    fn decode_envelope(&self, buf: &[u8]) -> Option<MethodCallResult> {
        let mut reader = Reader::new(buf);
        let n = reader.read_u8();
        if n == 0 {
            let ret = StandardMethodCodec::read_value(&mut reader).unwrap();
            Some(MethodCallResult::Ok(ret))
        } else if n == 1 {
            let code = StandardMethodCodec::read_value(&mut reader).unwrap();
            let message = StandardMethodCodec::read_value(&mut reader).unwrap();
            let details = StandardMethodCodec::read_value(&mut reader).unwrap();
            Some(MethodCallResult::Err {
                code: match code {
                    Value::String(s) => s,
                    _ => "".into(),
                },
                message: match message {
                    Value::String(s) => s,
                    _ => "".into(),
                },
                details,
            })
        } else {
            None
        }
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }
    fn read_u8(&mut self) -> u8 {
        let n = self.buf[self.pos];
        self.pos += 1;
        n
    }
    fn read_u16(&mut self) -> u16 {
        self.pos += 2;
        let s = &self.buf[self.pos - 2..self.pos];
        u16::from_ne_bytes(clone_into_array(s))
    }
    fn read_u32(&mut self) -> u32 {
        self.pos += 4;
        let s = &self.buf[self.pos - 4..self.pos];
        u32::from_ne_bytes(clone_into_array(s))
    }
    fn read_i32(&mut self) -> i32 {
        self.pos += 4;
        let s = &self.buf[self.pos - 4..self.pos];
        i32::from_ne_bytes(clone_into_array(s))
    }
    fn read_u64(&mut self) -> u64 {
        self.pos += 8;
        let s = &self.buf[self.pos - 8..self.pos];
        u64::from_ne_bytes(clone_into_array(s))
    }
    fn read_i64(&mut self) -> i64 {
        self.pos += 8;
        let s = &self.buf[self.pos - 8..self.pos];
        i64::from_ne_bytes(clone_into_array(s))
    }
    fn read_f64(&mut self) -> f64 {
        let n = self.read_u64();
        unsafe { mem::transmute::<u64, f64>(n) }
    }
    fn read_size(&mut self) -> usize {
        let n = self.read_u8();
        match n {
            254 => self.read_u16() as usize,
            255 => self.read_u32() as usize,
            _ => n as usize,
        }
    }
    fn read_string(&mut self, len: usize) -> String {
        unsafe {
            if len == 0 {
                String::from("")
            } else {
                let v = slice::from_raw_parts(&self.buf[self.pos], len);
                self.pos += len;
                String::from_utf8_lossy(v).to_owned().to_string()
            }
        }
    }
    fn read_u8_list(&mut self, len: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(len);
        for _ in 0..len {
            v.push(self.read_u8());
        }
        v
    }
    fn read_i32_list(&mut self, len: usize) -> Vec<i32> {
        let mut v = Vec::with_capacity(len);
        for _ in 0..len {
            v.push(self.read_i32());
        }
        v
    }
    fn read_i64_list(&mut self, len: usize) -> Vec<i64> {
        let mut v = Vec::with_capacity(len);
        for _ in 0..len {
            v.push(self.read_i64());
        }
        v
    }
    fn read_f64_list(&mut self, len: usize) -> Vec<f64> {
        let mut v = Vec::with_capacity(len);
        for _ in 0..len {
            let n = self.read_i64();
            v.push(unsafe { mem::transmute::<i64, f64>(n) });
        }
        v
    }
    fn ended(&self) -> bool {
        self.pos >= self.buf.len()
    }
}

struct Writer(Vec<u8>);

impl Writer {
    fn new(v: Vec<u8>) -> Self {
        Writer(v)
    }
    fn write_u8(&mut self, n: u8) {
        self.0.push(n);
    }
    fn write_u16(&mut self, n: u16) {
        self.0.extend_from_slice(&n.to_ne_bytes());
    }
    fn write_u32(&mut self, n: u32) {
        self.0.extend_from_slice(&n.to_ne_bytes());
    }
    fn write_i32(&mut self, n: i32) {
        self.0.extend_from_slice(&n.to_ne_bytes());
    }
    fn write_u64(&mut self, n: u64) {
        self.0.extend_from_slice(&n.to_ne_bytes());
    }
    fn write_i64(&mut self, n: i64) {
        self.0.extend_from_slice(&n.to_ne_bytes());
    }
    fn write_f64(&mut self, n: f64) {
        self.write_u64(unsafe { mem::transmute::<f64, u64>(n) });
    }
    fn write_size(&mut self, n: usize) {
        if n < 254 {
            self.write_u8(n as u8);
        } else if n <= u16::max_value() as usize {
            self.write_u8(254);
            self.write_u16(n as u16);
        } else if n < u32::max_value() as usize {
            self.write_u8(255);
            self.write_u32(n as u32);
        } else {
            // flutter only support 32 bit value
            panic!("Not implemented");
        }
    }
    fn write_string(&mut self, s: &str) {
        self.0.extend_from_slice(s.as_bytes());
    }
    fn align_to(&mut self, align: u8) {
        let m = self.0.len() % align as usize;
        for _ in 0..m {
            self.write_u8(0);
        }
    }
    fn write_buf(&mut self, list: &[u8]) {
        self.0.extend_from_slice(list);
    }
}

use std::convert::AsMut;

fn clone_into_array<A, T>(slice: &[T]) -> A
where
    A: Sized + Default + AsMut<[T]>,
    T: Clone,
{
    let mut a = Default::default();
    <A as AsMut<[T]>>::as_mut(&mut a).clone_from_slice(slice);
    a
}
