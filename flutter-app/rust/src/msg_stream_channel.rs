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
    iter::repeat,
    sync::{ Arc, Mutex },
    time::{ Duration },
};
use tokio::prelude::*;
use stream_cancel::{StreamExt as StreamExt2, Tripwire, Trigger};
use log::{info};

const CHANNEL_NAME: &str = "rust/msg_stream";

pub struct MsgStreamPlugin {
    channel: Arc<Mutex<StandardMethodChannel>>,
    stop_trigger: Option<Trigger>,
}

impl MsgStreamPlugin {
    pub fn new() -> MsgStreamPlugin {
        MsgStreamPlugin {
            channel: Arc::new(Mutex::new(StandardMethodChannel::new(CHANNEL_NAME))),
            stop_trigger: None,
        }
    }
}

impl Plugin for MsgStreamPlugin {
    fn init_channel(&self, registry: &PluginRegistry) -> &str {
        let channel = self.channel.lock().unwrap();
        channel.init(registry);
        CHANNEL_NAME
    }

    fn handle(&mut self, msg: &PlatformMessage, engine: &FlutterEngineInner, _window: &mut Window) {
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

                let c = self.channel.clone();
                let (trigger, tripwire) = Tripwire::new();
                self.stop_trigger = Some(trigger);

                engine.with_rt(|rt| {
                    rt.spawn(futures::lazy(|| {
                        let v = vec![
                            "Hello?",
                            "What's your name?",
                            "How old are you?",
                            "Maybe we can be friend together...",
                            "Do you have a brother or sister?"
                        ];
                        stream::iter_ok::<_, ()>(repeat(v).flatten())
                            .throttle(Duration::from_secs(1))
                            .map_err(|e| eprintln!("Error = {:?}", e))
                            .take_until(tripwire)
                            .for_each(move |v| {
                                let channel = c.lock().unwrap();
                                // Do I need to dispatch this call to GUI thread?
                                let ret = Value::String(String::from(v));
                                channel.send_success_event(&ret);
                                println!("v: {}", v);
                                Ok(())
                            })
                    }));
                });
            },
            "cancel" => {
                // drop the trigger to stop stream
                self.stop_trigger.take();
                channel.send_method_call_response(
                    msg.response_handle,
                    MethodCallResult::Ok(Value::Null)
                );
            },
            _ => (),
        }
    }
}