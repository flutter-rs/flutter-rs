use flutter_engine::{
    Window,
    codec::{
        MethodCodec,
        standard_codec::{
            Value,
            StandardMethodCodec
        }
    },
    PlatformMessage,
    FlutterEngineInner,
    plugins::Plugin,
    channel::{ Channel, StandardMethodChannel },
};

pub struct CalcPlugin {
    channel: StandardMethodChannel,
}

impl CalcPlugin {
    pub fn new() -> CalcPlugin {
        CalcPlugin {
            channel: StandardMethodChannel::new("rust/calc")
        }
    }
}

impl Plugin for CalcPlugin {
    fn get_channel_mut(&mut self) -> &mut Channel {
        return &mut self.channel;
    }
    fn handle(&mut self, msg: &PlatformMessage, engine: &FlutterEngineInner, _window: &mut Window) {
        let decoded = self.channel.decode_method_call(msg);
        match decoded.method.as_str() {
            "fibonacci" => {
                // TODO: what if we want this processor to be async? we need to cache engine and handle?
                if let Value::I32(n) = decoded.args {
                    if n > 0 {
                        let ret = fibonacci(n);
                        if let Some(response_handle) = msg.response_handle {
                            let buf = if let Some(v) = ret {
                                StandardMethodCodec::encode_success_envelope(&Value::I32(v))
                            } else {
                                StandardMethodCodec::encode_error_envelope("100", "Overflow", &Value::Null)
                            };
                            engine.send_platform_message_response(
                                response_handle,
                                &buf,
                            );
                        }
                    }
                }
            },
            _ => (),
        }
    }
}

// TODO: we can move this to a async context and do the calc
fn fibonacci(n: i32) -> Option<i32> {
    if n <= 0 {
        return Some(0);
    }
    let mut a = 0i32;
    let mut b = 1i32;
    let mut i = 0;
    while n > i + 1 {
        if let Some(t) = a.checked_add(b) {
            a = b;
            b = t;
            i += 1;
        } else {
            return None;
        }
    }
    Some(b)
}