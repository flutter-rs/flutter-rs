use flutter_engine::plugins::prelude::*;

const PLUGIN_NAME: &str = module_path!();
const CHANNEL_NAME: &str = "rust/calc";

pub struct CalcPlugin {
    handler: Arc<RwLock<Handler>>,
}

struct Handler;

impl CalcPlugin {
    pub fn new() -> Self {
        Self {
            handler: Arc::new(RwLock::new(Handler)),
        }
    }
}

impl Plugin for CalcPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, registrar: &mut ChannelRegistrar) {
        let method_handler = Arc::downgrade(&self.handler);
        registrar.register_channel(StandardMethodChannel::new(CHANNEL_NAME, method_handler));
    }
}

impl MethodCallHandler for Handler {
    fn on_method_call(
        &mut self,
        call: MethodCall,
        _: RuntimeData,
    ) -> Result<Value, MethodCallError> {
        match call.method.as_str() {
            "fibonacci" => {
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
