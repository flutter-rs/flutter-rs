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
use std::{
    sync::{ Arc, Mutex },
    time::{ Duration },
};
use log::{info};

const CHANNEL_NAME: &str = "rust/msg_stream";

pub struct MsgStreamPlugin {
    channel: Arc<Mutex<StandardMethodChannel>>,
}

impl MsgStreamPlugin {
    pub fn new() -> MsgStreamPlugin {
        MsgStreamPlugin {
            channel: Arc::new(Mutex::new(StandardMethodChannel::new(CHANNEL_NAME))),
        }
    }
}

impl Plugin for MsgStreamPlugin {
    fn init_channel(&self, registry: &PluginRegistry) -> &str {
        let channel = self.channel.lock().unwrap();
        channel.init(registry);
        CHANNEL_NAME
    }

    fn handle(&mut self, msg: &PlatformMessage, _engine: &FlutterEngineInner, _window: &mut Window) {
        let channel = self.channel.lock().unwrap();
        let decoded = channel.decode_method_call(msg);
        info!("Got method call {}", decoded.method);
        match decoded.method.as_str() {
            "listen" => {
                if let Value::I32(n) = decoded.args {
                    info!("Random stream invoked with params {}", n);
                }

                channel.send_method_call_response(
                    msg.response_handle,
                    MethodCallResult::Ok(Value::Null)
                );

                let ret = Value::String(String::from("Hello?"));
                channel.send_success_event(&ret);

                let channel = self.channel.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(1));
                    let channel = channel.lock().unwrap();
                    let ret1 = Value::String(String::from("What's your name?"));
                    channel.send_success_event(&ret1);
                });

                let channel = self.channel.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(2));
                    let channel = channel.lock().unwrap();
                    let ret2 = Value::String(String::from("Nice to see you, man!"));
                    channel.send_success_event(&ret2);
                });
            },
            "cancel" => {
                channel.send_method_call_response(
                    msg.response_handle,
                    MethodCallResult::Ok(Value::Null)
                );
            },
            _ => (),
        }
    }
}