//! Plugin to handle system dialogs.
//! It handles flutter-rs/dialog type message.

use crate::{FlutterEngineInner};
use super::{Plugin, PlatformMessage, PluginRegistry, ffi::FlutterPlatformMessageResponseHandle};
use channel::{ Channel, JsonMethodChannel };
use codec::MethodCallResult;
use std::sync::{ Arc, Mutex };
use serde_json::Value;

const CHANNEL_NAME: &str = "flutter-rs/window";

pub struct WindowPlugin {
    channel: Arc<Mutex<JsonMethodChannel>>,
    // handle: Handle,
}

impl WindowPlugin {
    pub fn new() -> Self {
        WindowPlugin {
            channel: Arc::new(Mutex::new(JsonMethodChannel::new(CHANNEL_NAME)))
        }
    }
}

impl Plugin for WindowPlugin {
    fn init_channel(&self, registry: &PluginRegistry) -> &str {
        let channel = self.channel.lock().unwrap();
        channel.init(registry);
        CHANNEL_NAME
    }
    fn handle(&mut self, msg: &PlatformMessage, _engine: Arc<FlutterEngineInner>, window: &mut glfw::Window) {
        let handle = msg.get_response_handle();
        let channel = self.channel.lock().unwrap();
        let decoded = channel.decode_method_call(msg);

        let s = serde_json::to_string(&decoded.args);

        match decoded.method.as_str() {
            "maximize" => {
                window.maximize();
            },
            "iconify" => {
                window.iconify();
            },
            "restore" => {
                window.restore();
            },
            "show" => {
                window.show();
            },
            "hide" => {
                window.hide();
            },
            "close" => {
                window.set_should_close(true);
            },
            "set_pos" => {
                let params: serde_json::Result<PositionParams> = serde_json::from_str(&s.unwrap());
                if params.is_err() {
                    channel.send_method_call_response(
                        handle.map(|h| {
                            unsafe {
                                &*(h as *const FlutterPlatformMessageResponseHandle)
                            }
                        }),
                        MethodCallResult::Err{
                            code: "1002".to_owned(), // TODO: put errors together
                            message: "Params error".to_owned(),
                            details: Value::Null,
                        },
                    );
                    return;
                }

                let params = params.unwrap();
                let PositionParams { x, y } = params;
                window.set_pos(x as i32, y as i32);
            },
            "get_pos" => {
                let (xpos, ypos) = window.get_pos();
                channel.send_method_call_response(
                    handle.map(|h| {
                        unsafe {
                            &*(h as *const FlutterPlatformMessageResponseHandle)
                        }
                    }),
                    MethodCallResult::Ok(json!({"x": xpos, "y": ypos})),
                );
                return;
            },
            _ => {
            }
        }

        channel.send_method_call_response(
            handle.map(|h| {
                unsafe {
                    &*(h as *const FlutterPlatformMessageResponseHandle)
                }
            }),
            MethodCallResult::Ok(Value::Null),
        );
    }
}

#[derive(Serialize, Deserialize)]
struct PositionParams {
    x: f32,
    y: f32,
}