use std::{
    iter::repeat,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use flutter_engine::{
    channel::{Channel, StandardMethodChannel},
    codec::{standard_codec::Value, MethodCallResult},
    plugins::{Plugin, PluginChannel},
    PlatformMessage, RuntimeData, Window,
};
use log::info;
use stream_cancel::{StreamExt as StreamExt2, Trigger, Tripwire};
use tokio::{prelude::*, runtime::TaskExecutor};

const CHANNEL_NAME: &str = "rust/msg_stream";

pub struct MsgStreamPlugin {
    channel: Arc<Mutex<StandardMethodChannel>>,
    stop_trigger: Option<Trigger>,
    executor: TaskExecutor,
}

impl PluginChannel for MsgStreamPlugin {
    fn channel_name() -> &'static str {
        CHANNEL_NAME
    }
}

impl MsgStreamPlugin {
    pub fn new(executor: TaskExecutor) -> MsgStreamPlugin {
        MsgStreamPlugin {
            channel: Arc::new(Mutex::new(StandardMethodChannel::new(CHANNEL_NAME))),
            stop_trigger: None,
            executor,
        }
    }
}

impl Plugin for MsgStreamPlugin {
    fn init_channel(&mut self, registry: Weak<RuntimeData>) {
        let mut channel = self.channel.lock().unwrap();
        channel.init(registry);
    }

    fn handle(&mut self, msg: &PlatformMessage, _window: &mut Window) {
        let channel = self.channel.lock().unwrap();
        let decoded = channel.decode_method_call(msg).unwrap();

        info!("Got method call {}", decoded.method);
        match decoded.method.as_str() {
            "listen" => {
                if let Value::I32(n) = decoded.args {
                    info!("Random stream invoked with params {}", n);
                }

                channel.send_method_call_response(
                    msg.response_handle.unwrap(),
                    MethodCallResult::Ok(Value::Null),
                );

                let (trigger, tripwire) = Tripwire::new();
                self.stop_trigger = Some(trigger);

                let channel = Arc::clone(&self.channel);
                self.executor.spawn(futures::lazy(move || {
                    let v = vec![
                        "Hello?",
                        "What's your name?",
                        "How old are you?",
                        "Maybe we can be friend together...",
                        "Do you have a brother or sister?",
                    ];
                    stream::iter_ok::<_, ()>(repeat(v).flatten())
                        .throttle(Duration::from_secs(1))
                        .map_err(|e| eprintln!("Error = {:?}", e))
                        .take_until(tripwire)
                        .for_each(move |v| {
                            let ret = Value::String(String::from(v));
                            let channel = channel.lock().unwrap();
                            channel.send_success_event(&ret);
                            Ok(())
                        })
                }));
            }
            "cancel" => {
                // drop the trigger to stop stream
                self.stop_trigger.take();
                channel.send_method_call_response(
                    msg.response_handle.unwrap(),
                    MethodCallResult::Ok(Value::Null),
                );
            }
            _ => (),
        }
    }
}
