use glutin::event::VirtualKeyCode as Key;

// Emulates glfw key numbers
// https://github.com/flutter/flutter/blob/master/packages/flutter/lib/src/services/keyboard_maps.dart
pub fn raw_key(key: Option<Key>) -> Option<u32> {
    let key = if let Some(key) = key {
        if key as u32 >= Key::A as u32 && key as u32 <= Key::Z as u32 {
            return Some(key as u32 - Key::A as u32 + 65);
        }

        if key as u32 >= Key::Key1 as u32 && key as u32 <= Key::Key9 as u32 {
            return Some(key as u32 - Key::Key1 as u32 + 49);
        }

        key
    } else {
        return None;
    };

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
