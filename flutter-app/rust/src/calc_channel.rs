use flutter_engine::{
    Window,
    PluginRegistry,
    codec::{
        MethodCallResult,
        standard_codec::{
            Value,
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
    fn init_channel(&self, registry: &PluginRegistry) -> &str {
        self.channel.init(registry);
        self.channel.get_name()
    }
    fn handle(&mut self, msg: &PlatformMessage, _engine: &FlutterEngineInner, _window: &mut Window) {
        let decoded = self.channel.decode_method_call(msg);
        match decoded.method.as_str() {
            "fibonacci" => {
                // TODO: what if we want this processor to be async? we need to cache engine and handle?
                let result = if let Value::String(s) = decoded.args {
                    if let Ok(n) = s.parse() {
                        if n >= 0 {
                            let ret = fibonacci(n);
                            if let Some(v) = ret {
                                MethodCallResult::Ok(Value::I64(v))
                            } else {
                                MethodCallResult::Err {
                                    code: "100".to_owned(),
                                    message: "Overflow".to_owned(),
                                    details: Value::Null,
                                }
                            }
                        } else {
                            MethodCallResult::Err {
                                code: "101".to_owned(),
                                message: "Minus!".to_owned(),
                                details: Value::Null,
                            }
                        }
                    } else {
                        MethodCallResult::Err {
                            code: "102".to_owned(),
                            message: "Not a number!".to_owned(),
                            details: Value::Null,
                        }
                    }
                } else {
                    MethodCallResult::Err {
                        code: "103".to_owned(),
                        message: "Format error".to_owned(),
                        details: Value::Null,
                    }
                };
                self.channel.send_method_call_response(
                    msg.response_handle,
                    result);
            },
            _ => (),
        }
    }
}

// TODO: we can move this to a async context and do the calc
fn fibonacci(n: i64) -> Option<i64> {
    if n <= 0 {
        return Some(0);
    }
    let mut a = 0i64;
    let mut b = 1i64;
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