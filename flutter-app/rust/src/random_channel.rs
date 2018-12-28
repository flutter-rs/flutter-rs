use flutter_engine::{
    Window,
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

use log::{info};

pub struct RandomPlugin {
    channel: StandardMethodChannel,
}

impl RandomPlugin {
    pub fn new() -> RandomPlugin {
        RandomPlugin {
            channel: StandardMethodChannel::new("rust/random")
        }
    }
}

impl Plugin for RandomPlugin {
    fn get_channel_mut(&mut self) -> &mut Channel {
        return &mut self.channel;
    }
    fn handle(&mut self, msg: &PlatformMessage, _engine: &FlutterEngineInner, _window: &mut Window) {
        let decoded = self.channel.decode_method_call(msg);
        match decoded.method.as_str() {
            "listen" => {
                // TODO: what if we want this processor to be async? we need to cache engine and handle?
                if let Value::I32(n) = decoded.args {
                    info!("Got invoked with {} has handle? {}", n, msg.response_handle.is_some());
                }

                self.channel.send_method_call_response(
                    msg.response_handle,
                    MethodCallResult::Ok(Value::Null)
                );
            },
            "cancel" => {

            },
            _ => (),
        }
    }
}