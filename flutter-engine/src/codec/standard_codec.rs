use codec::{ MethodCodec, MethodCall, MethodCallResult };
use std::{ u16, u32 };
use slice;

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
pub enum Value {
    Null,
    Boolean(bool),
    I32(i32),
    I64(i64),
    LargeInt(),
    F64(f64),
    String(String),
    U8List(Vec<u8>),
    I32List(Vec<i32>),
    I64List(Vec<i64>),
    F64List(Vec<f64>),
    //List(Vec<Value>),
    //Map(HashMap<Value, Value>),
}

#[derive(Debug)]
enum DecodeError {
    Ended
}

pub struct StandardMethodCodec;

impl StandardMethodCodec {
    fn read_value(reader: &mut Reader) -> Result<Value, DecodeError> {
        if reader.ended() {
            return Err(DecodeError::Ended)
        }

        let t = reader.read_u8();
        Ok(match t {
            VALUE_NULL => Value::Null,
            VALUE_FALSE => Value::Boolean(false),
            VALUE_TRUE => Value::Boolean(true),
            VALUE_INT32 => {
                Value::I32(reader.read_i32())
            },
            VALUE_INT64 => {
                Value::I64(reader.read_i64())
            },
            VALUE_LARGEINT => {
                panic!("Not implemented")
            },
            VALUE_FLOAT64 => {
                panic!("Not implemented")
            },
            VALUE_STRING => {
                let len = reader.read_size();
                Value::String(reader.read_string(len))
            },
            _ => Value::Null,
        })
    }
    fn write_value(writer: &mut Writer, v: &Value) {
        match v {
            Value::Null => {
                writer.write_u8(VALUE_NULL);
            },
            Value::Boolean(v) => {
                writer.write_u8(if *v { VALUE_TRUE } else { VALUE_FALSE });
            },
            Value::I32(n) => {
                writer.write_u8(VALUE_INT32);
                writer.write_i32(*n);
            }
            Value::String(s) => {
                writer.write_u8(VALUE_STRING);
                writer.write_size(s.len());
                writer.write_string(s);
            },
            _ => (),
        }
    }
}

impl MethodCodec for StandardMethodCodec {
    type R = Value;

    fn decode_method_call(buf: &[u8]) -> Option<MethodCall<Self::R>> {
        let mut reader = Reader::new(buf);
        let method: Value = StandardMethodCodec::read_value(&mut reader).unwrap();
        let args: Value = StandardMethodCodec::read_value(&mut reader).unwrap();

        if let Value::String(method) = method {
            return Some(MethodCall {
                method,
                args,
            });
        }
        error!("Invalid method call");
        None
    }

    fn decode_envelope(buf: &[u8]) -> Option<MethodCallResult<Self::R>> {
        None
    }
    
    fn encode_method_call(v: &MethodCall<Self::R>) -> Vec<u8> {
        vec![]
    }

    fn encode_success_envelope(result: &Self::R) -> Vec<u8> {
        let mut writer = Writer::new(Vec::new());
        writer.write_u8(0);
        StandardMethodCodec::write_value(&mut writer, result);
        writer.0
    }

    fn encode_error_envelope(code: &str, message: &str, v: &Self::R) -> Vec<u8> {
        let mut writer = Writer::new(Vec::new());
        writer.write_u8(1);
        StandardMethodCodec::write_value(&mut writer, &Value::String(code.to_owned()));
        StandardMethodCodec::write_value(&mut writer, &Value::String(message.to_owned()));
        StandardMethodCodec::write_value(&mut writer, v);
        writer.0
    }

}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

// TODO: use int_to_from_bytes when it's stablized
// currrent implementation use litte endiness
impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader {
            buf,
            pos: 0,
        }
    }
    fn read_u8(&mut self) -> u8 {
        let n = self.buf[self.pos];
        self.pos += 1;
        n
    }
    fn read_u16(&mut self) -> u16 {
        let mut n: u16 = 0;
        n |= (self.buf[self.pos + 0] as u16).rotate_left(0);
        n |= (self.buf[self.pos + 1] as u16).rotate_left(8);
        self.pos += 2;
        n
    }
    fn read_u32(&mut self) -> u32 {
        let mut n: u32 = 0;
        n |= (self.buf[self.pos + 0] as u32).rotate_left(0);
        n |= (self.buf[self.pos + 1] as u32).rotate_left(8);
        n |= (self.buf[self.pos + 2] as u32).rotate_left(16);
        n |= (self.buf[self.pos + 3] as u32).rotate_left(24);
        self.pos += 4;
        n
    }
    fn read_i32(&mut self) -> i32 {
        let mut n: i32 = 0;
        n |= (self.buf[self.pos + 0] as i32).rotate_left(0);
        n |= (self.buf[self.pos + 1] as i32).rotate_left(8);
        n |= (self.buf[self.pos + 2] as i32).rotate_left(16);
        n |= (self.buf[self.pos + 3] as i32).rotate_left(24);
        self.pos += 4;
        n
    }
    fn read_i64(&mut self) -> i64 {
        let mut n: i64 = 0;
        n |= (self.buf[self.pos + 0] as i64).rotate_left(0);
        n |= (self.buf[self.pos + 1] as i64).rotate_left(8);
        n |= (self.buf[self.pos + 2] as i64).rotate_left(16);
        n |= (self.buf[self.pos + 3] as i64).rotate_left(24);
        n |= (self.buf[self.pos + 4] as i64).rotate_left(32);
        n |= (self.buf[self.pos + 5] as i64).rotate_left(40);
        n |= (self.buf[self.pos + 6] as i64).rotate_left(48);
        n |= (self.buf[self.pos + 7] as i64).rotate_left(56);
        self.pos += 8;
        n
    }
    fn read_size(&mut self) -> usize {
        let n = self.read_u8();
        match n {
            254 => {
                self.read_u16() as usize
            },
            255 => {
                self.read_u32() as usize
            },
            _ => n as usize,
        }
    }
    fn read_string(&mut self, len: usize) -> String {
        unsafe {
            let v = slice::from_raw_parts(&self.buf[self.pos], len);
            self.pos += len;
            String::from_utf8_lossy(v).to_owned().to_string()
        }
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
    // TODO: Do I rotate with u8 or i8?
    fn write_i32(&mut self, n: i32) {
        self.0.push(n.rotate_right(0) as u8);
        self.0.push(n.rotate_right(8) as u8);
        self.0.push(n.rotate_right(16) as u8);
        self.0.push(n.rotate_right(24) as u8);
    }
    fn write_u16(&mut self, n: u16) {
        self.0.push(n.rotate_right(0) as u8);
        self.0.push(n.rotate_right(8) as u8);
    }
    fn write_u32(&mut self, n: u32) {
        self.0.push(n.rotate_right(0) as u8);
        self.0.push(n.rotate_right(8) as u8);
        self.0.push(n.rotate_right(16) as u8);
        self.0.push(n.rotate_right(24) as u8);
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
            // flutter only write 32 bit value
            panic!("Not implemented");
        }
    }
    fn write_string(&mut self, s: &str) {
        self.0.extend_from_slice(s.as_bytes());
    }
}