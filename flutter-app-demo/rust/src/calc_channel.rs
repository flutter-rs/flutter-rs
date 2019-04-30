use flutter_engine::plugins::prelude::*;

const PLUGIN_NAME: &str = module_path!();
const CHANNEL_NAME: &str = "rust/calc";

pub struct CalcPlugin {
    channel: Weak<StandardMethodChannel>,
}

impl CalcPlugin {
    pub fn new() -> Self {
        Self {
            channel: Weak::new(),
        }
    }
}

impl Plugin for CalcPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, plugin: Weak<RwLock<Self>>, registrar: &mut ChannelRegistrar) {
        self.channel = registrar.register_channel(StandardMethodChannel::new(CHANNEL_NAME, plugin));
    }
}

impl MethodCallHandler for CalcPlugin {
    fn on_method_call(
        &mut self,
        _channel: &str,
        call: MethodCall,
        _: &mut Window,
    ) -> Result<Value, MethodCallError> {
        match call.method.as_str() {
            "fibonacci" => {
                // TODO: what if we want this processor to be async? we need to cache engine and handle?
                if let Value::String(s) = call.args {
                    if let Ok(n) = s.parse() {
                        if n >= 0 {
                            if let Some(v) = fibonacci(n) {
                                Ok(Value::I64(v))
                            } else {
                                Err(MethodCallError::CustomError {
                                    code: "100".to_owned(),
                                    message: "Overflow".to_owned(),
                                    details: Value::Null,
                                })
                            }
                        } else {
                            Err(MethodCallError::CustomError {
                                code: "101".to_owned(),
                                message: "Minus!".to_owned(),
                                details: Value::Null,
                            })
                        }
                    } else {
                        Err(MethodCallError::CustomError {
                            code: "102".to_owned(),
                            message: "Not a number!".to_owned(),
                            details: Value::Null,
                        })
                    }
                } else {
                    Err(MethodCallError::CustomError {
                        code: "103".to_owned(),
                        message: "Format error".to_owned(),
                        details: Value::Null,
                    })
                }
            }
            _ => Err(MethodCallError::NotImplemented),
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
