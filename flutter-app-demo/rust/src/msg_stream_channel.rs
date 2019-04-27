use std::{
    iter::repeat,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use flutter_engine::{
    channel::{Channel, EventChannel},
    codec::{standard_codec::Value, MethodCallResult},
    plugins::{Plugin, PluginChannel},
    PlatformMessage, RuntimeData, Window,
};
use log::info;
use stream_cancel::{StreamExt as StreamExt2, Trigger, Tripwire};
use tokio::{prelude::*, runtime::TaskExecutor};

const CHANNEL_NAME: &str = "rust/msg_stream";

pub struct MsgStreamPlugin {
    channel: Arc<Mutex<EventChannel<MsgStreamPlugin>>>,
    stop_trigger: Option<Trigger>,
    executor: TaskExecutor,
}

impl PluginChannel for MsgStreamPlugin {
    fn channel_name() -> &'static str {
        CHANNEL_NAME
    }
}

impl MsgStreamPlugin {
    pub fn new(executor: TaskExecutor) -> Self {
        Self {
            channel: Arc::new(Mutex::new(EventChannel::new(
                CHANNEL_NAME,
                Self::on_listen,
                Self::on_cancel,
            ))),
            stop_trigger: None,
            executor,
        }
    }

    fn on_listen(&mut self, args: Value) -> MethodCallResult<Value> {
        if let Value::I32(n) = args {
            info!("Random stream invoked with params {}", n);
        }

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
        MethodCallResult::Ok(Value::Null)
    }

    fn on_cancel(&mut self) -> MethodCallResult<Value> {
        // drop the trigger to stop stream
        self.stop_trigger.take();
        MethodCallResult::Ok(Value::Null)
    }
}

impl Plugin for MsgStreamPlugin {
    fn init_channel(&mut self, registry: Weak<RuntimeData>) {
        let mut channel = self.channel.lock().unwrap();
        channel.init(registry);
    }

    fn handle(&mut self, msg: &mut PlatformMessage, _window: &mut Window) {
        let channel = Arc::clone(&self.channel);
        let channel = channel.lock().unwrap();
        channel.handle(self, msg);
    }
}
