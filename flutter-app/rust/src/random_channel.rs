use flutter_engine::{
    Window,
    codec::{
        MethodCodec,
        standard_codec::{
            Value,
            StandardMethodCodec
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
    fn handle(&mut self, msg: &PlatformMessage, engine: &FlutterEngineInner, _window: &mut Window) {
        let decoded = self.channel.decode_method_call(msg);
        match decoded.method.as_str() {
            "listen" => {
                // TODO: what if we want this processor to be async? we need to cache engine and handle?
                if let Value::I32(n) = decoded.args {
                    info!("Got invoked with {} has handle? {}", n, msg.response_handle.is_some());
                }

                if let Some(response_handle) = msg.response_handle {
                    let buf = StandardMethodCodec::encode_success_envelope(&Value::Null);
                    engine.send_platform_message_response(
                        response_handle,
                        &buf,
                    );
                }
            },
            "cancel" => {

            },
            _ => (),
        }
    }
}