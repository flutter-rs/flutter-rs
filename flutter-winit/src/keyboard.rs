use flutter_engine::FlutterEngine;
use flutter_plugins::keyevent::{KeyAction, KeyActionType, KeyEventPlugin};
use flutter_plugins::textinput::TextInputPlugin;
use winit::event::{ElementState, KeyboardInput, ModifiersState, VirtualKeyCode as Key};

pub struct Keyboard {
    engine: FlutterEngine,
    modifiers: u32,
}

impl Keyboard {
    pub fn new(engine: FlutterEngine) -> Self {
        Self {
            engine,
            modifiers: 0,
        }
    }

    pub fn set_modifiers(&mut self, modifiers: ModifiersState) {
        let shift = modifiers.shift() as u32;
        let ctrl = modifiers.ctrl() as u32;
        let alt = modifiers.alt() as u32;
        let logo = modifiers.logo() as u32;
        self.modifiers = shift | ctrl << 1 | alt << 2 | logo << 3;
    }

    pub fn input(&self, input: &KeyboardInput) {
        #[allow(deprecated)]
        let KeyboardInput {
            modifiers: _,
            virtual_keycode,
            scancode,
            state,
        } = input;
        let raw_key = if let Some(key) = virtual_keycode.and_then(raw_key) {
            key
        } else {
            return;
        };

        match state {
            ElementState::Pressed => {
                if let Some(key) = virtual_keycode {
                    self.engine
                        .with_plugin_mut(|text_input: &mut TextInputPlugin| match key {
                            Key::Return => {
                                text_input.with_state(|state| {
                                    state.add_characters(&"\n");
                                });
                                text_input.notify_changes();
                            }
                            Key::Back => {
                                text_input.with_state(|state| {
                                    state.backspace();
                                });
                                text_input.notify_changes();
                            }
                            _ => {}
                        });
                }

                self.engine
                    .with_plugin_mut(|keyevent: &mut KeyEventPlugin| {
                        keyevent.key_action(KeyAction {
                            toolkit: "glfw".to_string(),
                            key_code: raw_key as _,
                            scan_code: *scancode as _,
                            modifiers: self.modifiers as _,
                            keymap: "linux".to_string(),
                            _type: KeyActionType::Keydown,
                        });
                    });
            }
            ElementState::Released => {
                self.engine
                    .with_plugin_mut(|keyevent: &mut KeyEventPlugin| {
                        keyevent.key_action(KeyAction {
                            toolkit: "glfw".to_string(),
                            key_code: raw_key as _,
                            scan_code: *scancode as _,
                            modifiers: self.modifiers as _,
                            keymap: "linux".to_string(),
                            _type: KeyActionType::Keyup,
                        });
                    });
            }
        }
    }

    pub fn character(&self, ch: char) {
        if !ch.is_control() {
            self.engine
                .with_plugin_mut(|text_input: &mut TextInputPlugin| {
                    text_input.with_state(|state| {
                        state.add_characters(&ch.to_string());
                    });
                    text_input.notify_changes();
                });
        }
    }
}

// Emulates glfw key numbers
// https://github.com/flutter/flutter/blob/master/packages/flutter/lib/src/services/keyboard_maps.dart
fn raw_key(key: Key) -> Option<u32> {
    if key as u32 >= Key::A as u32 && key as u32 <= Key::Z as u32 {
        return Some(key as u32 - Key::A as u32 + 65);
    }

    if key as u32 >= Key::Key1 as u32 && key as u32 <= Key::Key9 as u32 {
        return Some(key as u32 - Key::Key1 as u32 + 49);
    }

    let code = match key {
        Key::Key0 => 48,
        Key::Return => 257,
        Key::Escape => 256,
        Key::Back => 259,
        Key::Tab => 258,
        Key::Space => 32,
        Key::LControl => 341,
        Key::LShift => 340,
        Key::LAlt => 342,
        Key::LWin => 343,
        Key::RControl => 345,
        Key::RShift => 344,
        Key::RAlt => 346,
        Key::RWin => 347,
        Key::Minus => 45,
        Key::Equals => 61,
        Key::LBracket => 91,
        Key::RBracket => 93,
        Key::Backslash => 92,
        Key::Semicolon => 59,
        Key::Apostrophe => 39,
        //Key::Backquote => 96,
        Key::Comma => 44,
        Key::Period => 46,
        Key::Slash => 47,
        //Key::CapsLock => 280,
        Key::Snapshot => 283,
        Key::Pause => 284,
        Key::Insert => 260,
        Key::Home => 268,
        Key::PageUp => 266,
        Key::Delete => 261,
        Key::End => 269,
        Key::PageDown => 267,
        Key::Right => 262,
        Key::Left => 263,
        Key::Down => 264,
        Key::Up => 265,
        Key::Numlock => 282,
        //Key::NumpadDivide => 331,
        //Key::NumpadMultiply => 332,
        //Key::NumpadAdd => 334,
        Key::NumpadEnter => 335,
        Key::Numpad0 => 320,
        Key::Numpad1 => 321,
        Key::Numpad2 => 322,
        Key::Numpad3 => 323,
        Key::Numpad4 => 324,
        Key::Numpad5 => 325,
        Key::Numpad6 => 326,
        Key::Numpad7 => 327,
        Key::Numpad8 => 328,
        Key::Numpad9 => 329,
        //Key::NumpadDecimal => 330,
        //Key::ContextMenu => 348,
        Key::NumpadEquals => 336,
        Key::F1 => 290,
        Key::F2 => 291,
        Key::F3 => 292,
        Key::F4 => 293,
        Key::F5 => 294,
        Key::F6 => 295,
        Key::F7 => 296,
        Key::F8 => 297,
        Key::F9 => 298,
        Key::F10 => 299,
        Key::F11 => 300,
        Key::F12 => 301,
        Key::F13 => 302,
        Key::F14 => 303,
        Key::F15 => 304,
        Key::F16 => 305,
        Key::F17 => 306,
        Key::F18 => 307,
        Key::F19 => 308,
        Key::F20 => 309,
        Key::F21 => 310,
        Key::F22 => 311,
        Key::F23 => 312,
        _ => return None,
    };
    Some(code)
}
