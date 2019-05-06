use std::{iter::repeat, time::Duration};

use flutter_engine::plugins::prelude::*;
use log::info;
use stream_cancel::{StreamExt as StreamExt2, Trigger, Tripwire};
use tokio::{prelude::*, runtime::TaskExecutor};

const PLUGIN_NAME: &str = module_path!();
const CHANNEL_NAME: &str = "rust/msg_stream";

pub struct MsgStreamPlugin {
    channel: Weak<EventChannel>,
    handler: Arc<RwLock<Handler>>,
}

impl Plugin for MsgStreamPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, registrar: &mut ChannelRegistrar) {
        let event_handler = Arc::downgrade(&self.handler);
        self.channel = registrar.register_channel(EventChannel::new(CHANNEL_NAME, event_handler));
    }
}

impl MsgStreamPlugin {
    pub fn new() -> Self {
        Self {
            channel: Weak::new(),
            handler: Arc::new(RwLock::new(Handler { stop_trigger: None })),
        }
    }
}

struct Handler {
    stop_trigger: Option<Trigger>,
}

impl EventHandler for Handler {
    fn on_listen(
        &mut self,
        args: Value,
        runtime_data: RuntimeData,
    ) -> Result<Value, MethodCallError> {
        if let Value::I32(n) = args {
            info!("Random stream invoked with params {}", n);
        }

        let (trigger, tripwire) = Tripwire::new();
        self.stop_trigger = Some(trigger);

        let rt = runtime_data.clone();
        runtime_data.task_executor.spawn(futures::lazy(move || {
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
                    rt.with_channel(CHANNEL_NAME, move |channel| {
                        let ret = Value::String(String::from(v));
                        channel.send_success_event(&ret);
                    })
                    .unwrap();
                    Ok(())
                })
        }));
        Ok(Value::Null)
    }

    fn on_cancel(&mut self, _: RuntimeData) -> Result<Value, MethodCallError> {
        // drop the trigger to stop stream
        self.stop_trigger.take();
        Ok(Value::Null)
    }
}
