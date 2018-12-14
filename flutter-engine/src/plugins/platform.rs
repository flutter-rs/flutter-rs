use crate::{FlutterEngineInner};
use super::{Plugin, PlatformMessage};

#[derive(Default)]
pub struct PlatformPlugin {}

impl Plugin for PlatformPlugin {
    fn get_channel(&self) -> String {
        String::from("flutter/platform")
    }
    fn handle(&mut self, msg: &PlatformMessage, _engine: &FlutterEngineInner, window: &mut glfw::Window) {
        if msg.message.method == "SystemChrome.setApplicationSwitcherDescription" {
            // label and primaryColor
            window.set_title(msg.message.args.as_object().unwrap().get("label").unwrap().as_str().unwrap());
        }
    }
}
