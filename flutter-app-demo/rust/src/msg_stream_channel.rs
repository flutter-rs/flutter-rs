use std::{iter::repeat, sync::Weak, time::Duration};

use flutter_engine::plugins::prelude::*;
use log::info;
use stream_cancel::{StreamExt as StreamExt2, Trigger, Tripwire};
use tokio::{prelude::*, runtime::TaskExecutor};

const PLUGIN_NAME: &str = module_path!();
const CHANNEL_NAME: &str = "rust/msg_stream";

pub struct MsgStreamPlugin {
    channel: Weak<EventChannel>,
    stop_trigger: Option<Trigger>,
    executor: TaskExecutor,
}

impl Plugin for MsgStreamPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, plugin: Weak<RwLock<Self>>, registrar: &mut ChannelRegistrar) {
        self.channel = registrar.register_channel(EventChannel::new(CHANNEL_NAME, plugin));
    }
}

impl MsgStreamPlugin {
    pub fn new(executor: TaskExecutor) -> Self {
        Self {
            channel: Weak::new(),
            stop_trigger: None,
            executor,
        }
    }
}

impl EventHandler for MsgStreamPlugin {
    fn on_listen(&mut self, _channel: &str, args: Value) -> Result<Value, MethodCallError> {
        if let Value::I32(n) = args {
            info!("Random stream invoked with params {}", n);
        }

        let (trigger, tripwire) = Tripwire::new();
        self.stop_trigger = Some(trigger);

        let channel = self.channel.clone();
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
                    let channel = channel.upgrade().unwrap();
                    channel.send_success_event(&ret);
                    Ok(())
                })
        }));
        Ok(Value::Null)
    }

    fn on_cancel(&mut self, _channel: &str) -> Result<Value, MethodCallError> {
        // drop the trigger to stop stream
        self.stop_trigger.take();
        Ok(Value::Null)
    }
}
