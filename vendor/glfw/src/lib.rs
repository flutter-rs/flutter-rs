// Copyright 2013-2016 The GLFW-RS Developers. For a full listing of the authors,
// refer to the AUTHORS file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![crate_type = "lib"]
#![crate_type = "rlib"]
#![crate_type = "dylib"]
#![crate_name = "glfw"]
#![deny(
    rust_2018_compatibility,
    rust_2018_idioms,
    nonstandard_style,
    unused,
    future_incompatible,
    missing_copy_implementations,
    missing_debug_implementations,
    missing_abi,
    clippy::doc_markdown
)]
#![allow(non_upper_case_globals)]

//! An idiomatic wrapper for the GLFW library.
//!
//! # Example
//!
//! ~~~no_run
//! extern crate glfw;
//!
//! use glfw::{Action, Context, Key};
//!
//! fn main() {
//!    use glfw::fail_on_errors;
//! let mut glfw = glfw::init(fail_on_errors!()).unwrap();
//!
//!     // Create a windowed mode window and its OpenGL context
//!     let (mut window, events) = glfw.create_window(300, 300, "Hello this is window",
//! glfw::WindowMode::Windowed)         .expect("Failed to create GLFW window.");
//!
//!     // Make the window's context current
//!     window.make_current();
//!     window.set_key_polling(true);
//!
//!     // Loop until the user closes the window
//!     while !window.should_close() {
//!         // Swap front and back buffers
//!         window.swap_buffers();
//!
//!         // Poll for and process events
//!         glfw.poll_events();
//!         for (_, event) in glfw::flush_messages(&events) {
//!             println!("{:?}", event);
//!             match event {
//!                 glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
//!                     window.set_should_close(true)
//!                 },
//!                 _ => {},
//!             }
//!         }
//!     }
//! }
//! ~~~
//!
//! # Cargo Features
//!
//! Use the `vulkan` feature flag to enable all Vulkan functions and types.
//!
//! Use the `image` feature flag to enable use of the [`image`](https://github.com/PistonDevelopers/image) library for cursors and icons.
//!
//! Use the `all` feature flag to enable both at the same time.

// TODO: Document differences between GLFW and glfw-rs
pub mod ffi {
    pub use glfw_sys::*;
}
macro_rules! make_user_callback_functions {
    (
        doc -> $doc:literal,
        set -> $set:ident,
        unset -> $unset:ident,
        poll -> $poll:ident,
        callback_field -> $callback_field:ident,
        poll_field -> $poll_field:ident,
        glfw -> $glfw:ident,
        args -> ($($args:ty),*),
        secret -> $secret:ident
    ) => {

        #[doc = $doc]
        pub fn $set<T>(&mut self, callback: T)
        where T: FnMut(&mut Window, $($args),*) + 'static {
            unsafe {
                let callbacks = WindowCallbacks::get_callbacks(self.ptr);
                callbacks.$callback_field = Some(Box::new(callback));
                ffi::$glfw(self.ptr, Some(Self::$secret));
            }
        }

        #[doc = $doc]
        pub fn $unset(&mut self) {
            unsafe {
                let callbacks = WindowCallbacks::get_callbacks(self.ptr);
                callbacks.$callback_field = None;

                // We're removing the callback, if theres no polling either, set to null
                if !callbacks.$poll_field {
                    ffi::$glfw(self.ptr, None);
                }
            }
        }

        #[doc = $doc]
        pub fn $poll(&mut self, should_poll: bool) {
            unsafe {
                let callbacks = WindowCallbacks::get_callbacks(self.ptr);
                callbacks.$poll_field = should_poll;

                // If no polling and not custom callback, set glfw callback to null
                if should_poll {
                    ffi::$glfw(self.ptr, Some(Self::$secret));
                } else if callbacks.$callback_field.is_none() {
                    ffi::$glfw(self.ptr, None);
                }
            }
        }
    }
}

macro_rules! new_callback {
    (
        doc -> $doc:literal,
        set -> $set:ident,
        unset -> $unset:ident,
        poll -> $poll:ident,
        callback_field -> $callback_field:ident,
        poll_field -> $poll_field:ident,
        window_event -> $window_event:ident ($($args:ty),+),
        glfw -> $glfw:ident ($($glfw_arg_names:ident: $glfw_args:ty),*),
        convert_args -> ($($convert_args:expr),*),
        secret -> $secret:ident
    ) => {

        #[allow(unused_unsafe)]
        extern "C" fn $secret(glfw_window: *mut GLFWwindow, $($glfw_arg_names: $glfw_args),*) {
            unsafe {
                let callbacks = WindowCallbacks::get_callbacks(glfw_window);
                let window = &mut *callbacks.window_ptr;
                if let Some(func) = &mut callbacks.$callback_field {
                    func(window, $($convert_args),*);
                }
                if callbacks.$poll_field {
                    let event = (ffi::glfwGetTime() as f64, WindowEvent::$window_event($($convert_args),*));
                    if let Some(event) = callbacks::unbuffered::handle(glfw_window as WindowId, event) {
                        callbacks.sender.send(event);
                    }
                }
            }
        }

        make_user_callback_functions!(
            doc -> $doc,
            set -> $set,
            unset -> $unset,
            poll -> $poll,
            callback_field -> $callback_field,
            poll_field -> $poll_field,
            glfw -> $glfw,
            args -> ($($args),*),
            secret -> $secret
        );
    };
    (
        doc -> $doc:literal,
        set -> $set:ident,
        unset -> $unset:ident,
        poll -> $poll:ident,
        callback_field -> $callback_field:ident,
        poll_field -> $poll_field:ident,
        window_event -> $window_event:ident,
        glfw -> $glfw:ident ($($glfw_arg_names:ident: $glfw_args:ty),*),
        convert_args -> ($($convert_args:expr),*),
        secret -> $secret:ident
    ) => {

        #[allow(unused_unsafe)]
        extern "C" fn $secret(glfw_window: *mut GLFWwindow, $($glfw_arg_names: $glfw_args),*) {
            unsafe {
                let callbacks = WindowCallbacks::get_callbacks(glfw_window);
                let window = &mut *callbacks.window_ptr;
                if let Some(func) = &mut callbacks.$callback_field {
                    func(window);
                }
                if callbacks.$poll_field {
                    let event = (ffi::glfwGetTime() as f64, WindowEvent::$window_event);
                    if let Some(event) = callbacks::unbuffered::handle(glfw_window as WindowId, event) {
                        callbacks.sender.send(event);
                    }
                }
            }
        }

        make_user_callback_functions!(
            doc -> $doc,
            set -> $set,
            unset -> $unset,
            poll -> $poll,
            callback_field -> $callback_field,
            poll_field -> $poll_field,
            glfw -> $glfw,
            args -> (),
            secret -> $secret
        );
    }
}

#[cfg(feature = "log")]
#[macro_use]
extern crate log;
#[macro_use]
extern crate bitflags;
#[cfg(feature = "image")]
#[allow(unused)]
extern crate image;

#[cfg(feature = "raw-window-handle-v0-6")]
extern crate raw_window_handle_0_6 as raw_window_handle;

#[cfg(feature = "raw-window-handle-v0-5")]
extern crate raw_window_handle_0_5 as raw_window_handle;

use std::collections::VecDeque;
#[allow(unused)]
use std::ffi::*;
use std::ffi::{CStr, CString};
use std::marker::Send;
use std::ops::{Deref, DerefMut};
#[cfg(all(not(target_os = "emscripten")))]
#[allow(unused)]
use std::os::raw::c_void;
use std::os::raw::{c_char, c_double, c_float, c_int, c_ushort};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::{error, fmt, mem, ptr, slice};

#[cfg(feature = "raw-window-handle-v0-6")]
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle, RawWindowHandle, RawDisplayHandle
};
#[cfg(feature = "raw-window-handle-v0-5")]
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle, RawDisplayHandle, RawWindowHandle};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Alias to `MouseButton1`, supplied for improved clarity.
pub use self::MouseButton::Button1 as MouseButtonLeft;
/// Alias to `MouseButton2`, supplied for improved clarity.
pub use self::MouseButton::Button2 as MouseButtonRight;
/// Alias to `MouseButton3`, supplied for improved clarity.
pub use self::MouseButton::Button3 as MouseButtonMiddle;
use crate::ffi::GLFWwindow;

mod callbacks;

#[derive(Debug)]
#[repr(transparent)]
pub struct PWindow(Box<Window>);

impl PWindow {
    fn raw_ptr(&mut self) -> *mut Window {
        self.0.deref_mut()
    }
}

impl Deref for PWindow {
    type Target = Window;
    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl DerefMut for PWindow {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}

unsafe impl Send for PWindow {}

unsafe impl Sync for PWindow {}

// these are technically already implemented, but somehow this fixed a error in wgpu
#[cfg(feature = "raw-window-handle-v0-6")]
impl HasWindowHandle for PWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

#[cfg(feature = "raw-window-handle-v0-6")]
impl HasDisplayHandle for PWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

/// Unique identifier for a `Window`.
pub type WindowId = usize;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

/// Input actions.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Action {
    Release = ffi::GLFW_RELEASE,
    Press = ffi::GLFW_PRESS,
    Repeat = ffi::GLFW_REPEAT,
}

/// Input keys.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Key {
    Space = ffi::GLFW_KEY_SPACE,
    Apostrophe = ffi::GLFW_KEY_APOSTROPHE,
    Comma = ffi::GLFW_KEY_COMMA,
    Minus = ffi::GLFW_KEY_MINUS,
    Period = ffi::GLFW_KEY_PERIOD,
    Slash = ffi::GLFW_KEY_SLASH,
    Num0 = ffi::GLFW_KEY_0,
    Num1 = ffi::GLFW_KEY_1,
    Num2 = ffi::GLFW_KEY_2,
    Num3 = ffi::GLFW_KEY_3,
    Num4 = ffi::GLFW_KEY_4,
    Num5 = ffi::GLFW_KEY_5,
    Num6 = ffi::GLFW_KEY_6,
    Num7 = ffi::GLFW_KEY_7,
    Num8 = ffi::GLFW_KEY_8,
    Num9 = ffi::GLFW_KEY_9,
    Semicolon = ffi::GLFW_KEY_SEMICOLON,
    Equal = ffi::GLFW_KEY_EQUAL,
    A = ffi::GLFW_KEY_A,
    B = ffi::GLFW_KEY_B,
    C = ffi::GLFW_KEY_C,
    D = ffi::GLFW_KEY_D,
    E = ffi::GLFW_KEY_E,
    F = ffi::GLFW_KEY_F,
    G = ffi::GLFW_KEY_G,
    H = ffi::GLFW_KEY_H,
    I = ffi::GLFW_KEY_I,
    J = ffi::GLFW_KEY_J,
    K = ffi::GLFW_KEY_K,
    L = ffi::GLFW_KEY_L,
    M = ffi::GLFW_KEY_M,
    N = ffi::GLFW_KEY_N,
    O = ffi::GLFW_KEY_O,
    P = ffi::GLFW_KEY_P,
    Q = ffi::GLFW_KEY_Q,
    R = ffi::GLFW_KEY_R,
    S = ffi::GLFW_KEY_S,
    T = ffi::GLFW_KEY_T,
    U = ffi::GLFW_KEY_U,
    V = ffi::GLFW_KEY_V,
    W = ffi::GLFW_KEY_W,
    X = ffi::GLFW_KEY_X,
    Y = ffi::GLFW_KEY_Y,
    Z = ffi::GLFW_KEY_Z,
    LeftBracket = ffi::GLFW_KEY_LEFT_BRACKET,
    Backslash = ffi::GLFW_KEY_BACKSLASH,
    RightBracket = ffi::GLFW_KEY_RIGHT_BRACKET,
    GraveAccent = ffi::GLFW_KEY_GRAVE_ACCENT,
    World1 = ffi::GLFW_KEY_WORLD_1,
    World2 = ffi::GLFW_KEY_WORLD_2,

    Escape = ffi::GLFW_KEY_ESCAPE,
    Enter = ffi::GLFW_KEY_ENTER,
    Tab = ffi::GLFW_KEY_TAB,
    Backspace = ffi::GLFW_KEY_BACKSPACE,
    Insert = ffi::GLFW_KEY_INSERT,
    Delete = ffi::GLFW_KEY_DELETE,
    Right = ffi::GLFW_KEY_RIGHT,
    Left = ffi::GLFW_KEY_LEFT,
    Down = ffi::GLFW_KEY_DOWN,
    Up = ffi::GLFW_KEY_UP,
    PageUp = ffi::GLFW_KEY_PAGE_UP,
    PageDown = ffi::GLFW_KEY_PAGE_DOWN,
    Home = ffi::GLFW_KEY_HOME,
    End = ffi::GLFW_KEY_END,
    CapsLock = ffi::GLFW_KEY_CAPS_LOCK,
    ScrollLock = ffi::GLFW_KEY_SCROLL_LOCK,
    NumLock = ffi::GLFW_KEY_NUM_LOCK,
    PrintScreen = ffi::GLFW_KEY_PRINT_SCREEN,
    Pause = ffi::GLFW_KEY_PAUSE,
    F1 = ffi::GLFW_KEY_F1,
    F2 = ffi::GLFW_KEY_F2,
    F3 = ffi::GLFW_KEY_F3,
    F4 = ffi::GLFW_KEY_F4,
    F5 = ffi::GLFW_KEY_F5,
    F6 = ffi::GLFW_KEY_F6,
    F7 = ffi::GLFW_KEY_F7,
    F8 = ffi::GLFW_KEY_F8,
    F9 = ffi::GLFW_KEY_F9,
    F10 = ffi::GLFW_KEY_F10,
    F11 = ffi::GLFW_KEY_F11,
    F12 = ffi::GLFW_KEY_F12,
    F13 = ffi::GLFW_KEY_F13,
    F14 = ffi::GLFW_KEY_F14,
    F15 = ffi::GLFW_KEY_F15,
    F16 = ffi::GLFW_KEY_F16,
    F17 = ffi::GLFW_KEY_F17,
    F18 = ffi::GLFW_KEY_F18,
    F19 = ffi::GLFW_KEY_F19,
    F20 = ffi::GLFW_KEY_F20,
    F21 = ffi::GLFW_KEY_F21,
    F22 = ffi::GLFW_KEY_F22,
    F23 = ffi::GLFW_KEY_F23,
    F24 = ffi::GLFW_KEY_F24,
    F25 = ffi::GLFW_KEY_F25,
    Kp0 = ffi::GLFW_KEY_KP_0,
    Kp1 = ffi::GLFW_KEY_KP_1,
    Kp2 = ffi::GLFW_KEY_KP_2,
    Kp3 = ffi::GLFW_KEY_KP_3,
    Kp4 = ffi::GLFW_KEY_KP_4,
    Kp5 = ffi::GLFW_KEY_KP_5,
    Kp6 = ffi::GLFW_KEY_KP_6,
    Kp7 = ffi::GLFW_KEY_KP_7,
    Kp8 = ffi::GLFW_KEY_KP_8,
    Kp9 = ffi::GLFW_KEY_KP_9,
    KpDecimal = ffi::GLFW_KEY_KP_DECIMAL,
    KpDivide = ffi::GLFW_KEY_KP_DIVIDE,
    KpMultiply = ffi::GLFW_KEY_KP_MULTIPLY,
    KpSubtract = ffi::GLFW_KEY_KP_SUBTRACT,
    KpAdd = ffi::GLFW_KEY_KP_ADD,
    KpEnter = ffi::GLFW_KEY_KP_ENTER,
    KpEqual = ffi::GLFW_KEY_KP_EQUAL,
    LeftShift = ffi::GLFW_KEY_LEFT_SHIFT,
    LeftControl = ffi::GLFW_KEY_LEFT_CONTROL,
    LeftAlt = ffi::GLFW_KEY_LEFT_ALT,
    LeftSuper = ffi::GLFW_KEY_LEFT_SUPER,
    RightShift = ffi::GLFW_KEY_RIGHT_SHIFT,
    RightControl = ffi::GLFW_KEY_RIGHT_CONTROL,
    RightAlt = ffi::GLFW_KEY_RIGHT_ALT,
    RightSuper = ffi::GLFW_KEY_RIGHT_SUPER,
    Menu = ffi::GLFW_KEY_MENU,
    Unknown = ffi::GLFW_KEY_UNKNOWN,
}

/// Wrapper around `glfwGetKeyName`
pub fn get_key_name(key: Option<Key>, scancode: Option<Scancode>) -> Option<String> {
    unsafe {
        string_from_nullable_c_str(ffi::glfwGetKeyName(
            match key {
                Some(k) => k as c_int,
                None => ffi::GLFW_KEY_UNKNOWN,
            },
            scancode.unwrap_or(ffi::GLFW_KEY_UNKNOWN),
        ))
    }
}

/// Wrapper around `glfwGetKeyName`
#[deprecated(
    since = "0.16.0",
    note = "'key_name' can cause a segfault, use 'get_key_name' instead"
)]
pub fn key_name(key: Option<Key>, scancode: Option<Scancode>) -> String {
    unsafe {
        string_from_c_str(ffi::glfwGetKeyName(
            match key {
                Some(k) => k as c_int,
                None => ffi::GLFW_KEY_UNKNOWN,
            },
            scancode.unwrap_or(ffi::GLFW_KEY_UNKNOWN),
        ))
    }
}

/// Wrapper around `glfwGetKeyScancode`.
pub fn get_key_scancode(key: Option<Key>) -> Option<Scancode> {
    unsafe {
        match ffi::glfwGetKeyScancode(match key {
            Some(key) => key as c_int,
            None => ffi::GLFW_KEY_UNKNOWN,
        }) {
            ffi::GLFW_KEY_UNKNOWN => None,
            scancode => Some(scancode as Scancode),
        }
    }
}

impl Key {
    /// Wrapper around `glfwGetKeyName` without scancode
    #[deprecated(
        since = "0.16.0",
        note = "Key method 'name' can cause a segfault, use 'get_name' instead"
    )]
    pub fn name(&self) -> String {
        #[allow(deprecated)]
        key_name(Some(*self), None)
    }

    /// Wrapper around `glfwGetKeyName` without scancode
    pub fn get_name(&self) -> Option<String> {
        get_key_name(Some(*self), None)
    }

    /// Wrapper around `glfwGetKeyScancode`.
    pub fn get_scancode(&self) -> Option<Scancode> {
        get_key_scancode(Some(*self))
    }
}

/// Mouse buttons. The `MouseButtonLeft`, `MouseButtonRight`, and
/// `MouseButtonMiddle` aliases are supplied for convenience.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MouseButton {
    /// The left mouse button. A `MouseButtonLeft` alias is provided to improve clarity.
    Button1 = ffi::GLFW_MOUSE_BUTTON_1,
    /// The right mouse button. A `MouseButtonRight` alias is provided to improve clarity.
    Button2 = ffi::GLFW_MOUSE_BUTTON_2,
    /// The middle mouse button. A `MouseButtonMiddle` alias is provided to improve clarity.
    Button3 = ffi::GLFW_MOUSE_BUTTON_3,
    Button4 = ffi::GLFW_MOUSE_BUTTON_4,
    Button5 = ffi::GLFW_MOUSE_BUTTON_5,
    Button6 = ffi::GLFW_MOUSE_BUTTON_6,
    Button7 = ffi::GLFW_MOUSE_BUTTON_7,
    Button8 = ffi::GLFW_MOUSE_BUTTON_8,
}

impl MouseButton {
    /// Alias to `MouseButton1`, supplied for improved clarity.
    pub const Left: Self = MouseButton::Button1;
    /// Alias to `MouseButton2`, supplied for improved clarity.
    pub const Right: Self = MouseButton::Button2;
    /// Alias to `MouseButton3`, supplied for improved clarity.
    pub const Middle: Self = MouseButton::Button3;

    /// Converts from `i32`.
    pub fn from_i32(n: i32) -> Option<MouseButton> {
        if (0..=ffi::GLFW_MOUSE_BUTTON_LAST).contains(&n) {
            Some(unsafe { mem::transmute(n) })
        } else {
            None
        }
    }
}

/// Formats the type using aliases rather than the default variant names.
///
/// # Example
///
/// ~~~ignore
/// assert_eq(format!("{}", glfw::MouseButtonLeft), "MouseButton1");
/// assert_eq(format!("{}", glfw::DebugAliases(glfw::MouseButtonLeft)), "MouseButtonLeft");
/// ~~~
pub struct DebugAliases<T>(pub T);

impl fmt::Debug for DebugAliases<MouseButton> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let DebugAliases(button) = *self;
        match button {
            MouseButtonLeft => write!(f, "MouseButtonLeft"),
            MouseButtonRight => write!(f, "MouseButtonRight"),
            MouseButtonMiddle => write!(f, "MouseButtonMiddle"),
            button => button.fmt(f),
        }
    }
}

/// Tokens corresponding to various error types.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Error {
    NoError = ffi::GLFW_NO_ERROR,
    NotInitialized = ffi::GLFW_NOT_INITIALIZED,
    NoCurrentContext = ffi::GLFW_NO_CURRENT_CONTEXT,
    InvalidEnum = ffi::GLFW_INVALID_ENUM,
    InvalidValue = ffi::GLFW_INVALID_VALUE,
    OutOfMemory = ffi::GLFW_OUT_OF_MEMORY,
    ApiUnavailable = ffi::GLFW_API_UNAVAILABLE,
    VersionUnavailable = ffi::GLFW_VERSION_UNAVAILABLE,
    PlatformError = ffi::GLFW_PLATFORM_ERROR,
    FormatUnavailable = ffi::GLFW_FORMAT_UNAVAILABLE,
    NoWindowContext = ffi::GLFW_NO_WINDOW_CONTEXT,
    CursorUnavailable = ffi::GLFW_CURSOR_UNAVAILABLE,
    FeatureUnavailable = ffi::GLFW_FEATURE_UNAVAILABLE,
    FeatureUnimplemented = ffi::GLFW_FEATURE_UNIMPLEMENTED,
    PlatformUnavailable = ffi::GLFW_PLATFORM_UNAVAILABLE,
    Unknown(c_int),
}

impl Error {
    /// Converts a raw GLFW error code without assuming it is known by this crate.
    pub const fn from_raw(raw: c_int) -> Self {
        match raw {
            ffi::GLFW_NO_ERROR => Self::NoError,
            ffi::GLFW_NOT_INITIALIZED => Self::NotInitialized,
            ffi::GLFW_NO_CURRENT_CONTEXT => Self::NoCurrentContext,
            ffi::GLFW_INVALID_ENUM => Self::InvalidEnum,
            ffi::GLFW_INVALID_VALUE => Self::InvalidValue,
            ffi::GLFW_OUT_OF_MEMORY => Self::OutOfMemory,
            ffi::GLFW_API_UNAVAILABLE => Self::ApiUnavailable,
            ffi::GLFW_VERSION_UNAVAILABLE => Self::VersionUnavailable,
            ffi::GLFW_PLATFORM_ERROR => Self::PlatformError,
            ffi::GLFW_FORMAT_UNAVAILABLE => Self::FormatUnavailable,
            ffi::GLFW_NO_WINDOW_CONTEXT => Self::NoWindowContext,
            ffi::GLFW_CURSOR_UNAVAILABLE => Self::CursorUnavailable,
            ffi::GLFW_FEATURE_UNAVAILABLE => Self::FeatureUnavailable,
            ffi::GLFW_FEATURE_UNIMPLEMENTED => Self::FeatureUnimplemented,
            ffi::GLFW_PLATFORM_UNAVAILABLE => Self::PlatformUnavailable,
            raw => Self::Unknown(raw),
        }
    }

    /// Returns the raw GLFW error code represented by this value.
    pub const fn as_raw(self) -> c_int {
        match self {
            Self::NoError => ffi::GLFW_NO_ERROR,
            Self::NotInitialized => ffi::GLFW_NOT_INITIALIZED,
            Self::NoCurrentContext => ffi::GLFW_NO_CURRENT_CONTEXT,
            Self::InvalidEnum => ffi::GLFW_INVALID_ENUM,
            Self::InvalidValue => ffi::GLFW_INVALID_VALUE,
            Self::OutOfMemory => ffi::GLFW_OUT_OF_MEMORY,
            Self::ApiUnavailable => ffi::GLFW_API_UNAVAILABLE,
            Self::VersionUnavailable => ffi::GLFW_VERSION_UNAVAILABLE,
            Self::PlatformError => ffi::GLFW_PLATFORM_ERROR,
            Self::FormatUnavailable => ffi::GLFW_FORMAT_UNAVAILABLE,
            Self::NoWindowContext => ffi::GLFW_NO_WINDOW_CONTEXT,
            Self::CursorUnavailable => ffi::GLFW_CURSOR_UNAVAILABLE,
            Self::FeatureUnavailable => ffi::GLFW_FEATURE_UNAVAILABLE,
            Self::FeatureUnimplemented => ffi::GLFW_FEATURE_UNIMPLEMENTED,
            Self::PlatformUnavailable => ffi::GLFW_PLATFORM_UNAVAILABLE,
            Self::Unknown(raw) => raw,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = match *self {
            Error::NoError => "NoError",
            Error::NotInitialized => "NotInitialized",
            Error::NoCurrentContext => "NoCurrentContext",
            Error::InvalidEnum => "InvalidEnum",
            Error::InvalidValue => "InvalidValue",
            Error::OutOfMemory => "OutOfMemory",
            Error::ApiUnavailable => "ApiUnavailable",
            Error::VersionUnavailable => "VersionUnavailable",
            Error::PlatformError => "PlatformError",
            Error::FormatUnavailable => "FormatUnavailable",
            Error::NoWindowContext => "NoWindowContext",
            Error::CursorUnavailable => "CursorUnavailable",
            Error::FeatureUnavailable => "FeatureUnavailable",
            Error::FeatureUnimplemented => "FeatureUnimplemented",
            Error::PlatformUnavailable => "PlatformUnavailable",
            Error::Unknown(raw) => return write!(f, "Unknown({raw})"),
        };

        f.write_str(description)
    }
}

impl error::Error for Error {}

/// The function to be used with the `fail_on_errors!()` callback.
pub fn fail_on_errors(e: Error, description: String) {
    if e == Error::FormatUnavailable {
        // https://github.com/PistonDevelopers/glfw-rs/issues/581
        /*
        This error only triggers on window creation and get_clipboard_string.
        Both those function return None on erorr case, so, we can safely ignore this error.
        */
        return;
    }
    panic!("GLFW Error: {}", description);
}

/// A callback that triggers a task failure when an error is encountered.
#[macro_export]
macro_rules! fail_on_errors {
    () => {{
        |error, description| {
            fail_on_errors(error, description);
        }
    }};
}

#[cfg(feature = "log")]
/// The function to be used with the `LOG_ERRORS` callback.
pub fn log_errors(_: Error, description: String) {
    error!("GLFW Error: {}", description);
}

#[cfg(not(feature = "log"))]
/// The function to be used with the `LOG_ERRORS` callback.
pub fn log_errors(_: Error, description: String) {
    eprintln!("GLFW Error: {}", description);
}

/// A callback that logs each error as it is encountered without triggering a
/// task failure
#[macro_export]
macro_rules! log_errors {
    () => {{
        |error, description| {
            log_errors(error, description);
        }
    }};
}

/// When not using the `image` library, or if you just want to,
/// you can specify an image from its raw pixel data using this structure.
#[derive(Debug)]
pub struct PixelImage {
    /// Width of the image in pixels
    pub width: u32,
    /// Height of the image in pixels
    pub height: u32,
    /// Pixels are 4 bytes each, one byte for each RGBA subpixel.
    pub pixels: Vec<u32>,
}

/// Cursor modes.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CursorMode {
    Normal = ffi::GLFW_CURSOR_NORMAL,
    Hidden = ffi::GLFW_CURSOR_HIDDEN,
    Captured = ffi::GLFW_CURSOR_CAPTURED,
    Disabled = ffi::GLFW_CURSOR_DISABLED,
}

/// Standard cursors provided by GLFW
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum StandardCursor {
    /// The regular arrow cursor shape.
    Arrow = ffi::GLFW_ARROW_CURSOR,
    /// The text input I-beam cursor shape.
    IBeam = ffi::GLFW_IBEAM_CURSOR,
    /// The crosshair shape.
    Crosshair = ffi::GLFW_CROSSHAIR_CURSOR,
    /// The hand shape.
    Hand = ffi::GLFW_HAND_CURSOR,
    /// The horizontal resize arrow shape.
    HResize = ffi::GLFW_HRESIZE_CURSOR,
    /// The vertical resize arrow shape.
    VResize = ffi::GLFW_VRESIZE_CURSOR,
}

/// Represents a window cursor that can be used to display any
/// of the standard cursors or load a custom cursor from an image.
///
/// Note that the cursor object has a lifetime and will not display
/// correctly after it has been dropped.
#[derive(Debug)]
pub struct Cursor {
    ptr: *mut ffi::GLFWcursor,
}

impl Drop for Cursor {
    fn drop(&mut self) {
        unsafe { ffi::glfwDestroyCursor(self.ptr) }
    }
}

impl Cursor {
    /// Create a new cursor using `glfwCreateStandardCursor`
    pub fn standard(cursor: StandardCursor) -> Cursor {
        Cursor {
            ptr: unsafe { ffi::glfwCreateStandardCursor(cursor as c_int) },
        }
    }

    /// Creates a new cursor from the image provided via `glfwCreateCursor`
    ///
    /// Note that the cursor image will be the same size as the image provided,
    /// so scaling it beforehand may be required.
    ///
    /// The cursor hotspot is specified in pixels, relative to the upper-left
    /// corner of the cursor image. Like all other coordinate systems in GLFW,
    /// the X-axis points to the right and the Y-axis points down.
    #[cfg(feature = "image")]
    pub fn create(image: image::RgbaImage, x_hotspot: u32, y_hotspot: u32) -> Cursor {
        let (width, height) = image.dimensions();

        let image_data = image.into_vec();

        let glfw_image = ffi::GLFWimage {
            width: width as c_int,
            height: height as c_int,
            pixels: image_data.as_ptr() as _,
        };

        Cursor {
            ptr: unsafe {
                ffi::glfwCreateCursor(
                    &glfw_image as *const ffi::GLFWimage,
                    x_hotspot as c_int,
                    y_hotspot as c_int,
                )
            },
        }
    }

    /// Creates a new cursor from the `PixelImage` provided via `glfwCreateCursor`
    ///
    /// Note that the cursor image will be the same size as the image provided,
    /// so scaling it beforehand may be required.
    ///
    /// The cursor hotspot is specified in pixels, relative to the upper-left
    /// corner of the cursor image. Like all other coordinate systems in GLFW,
    /// the X-axis points to the right and the Y-axis points down.
    pub fn create_from_pixels(image: PixelImage, x_hotspot: u32, y_hotspot: u32) -> Cursor {
        let glfw_image = ffi::GLFWimage {
            width: image.width as c_int,
            height: image.height as c_int,
            pixels: image.pixels.as_ptr() as _,
        };

        Cursor {
            ptr: unsafe {
                ffi::glfwCreateCursor(
                    &glfw_image as *const ffi::GLFWimage,
                    x_hotspot as c_int,
                    y_hotspot as c_int,
                )
            },
        }
    }
}

/// Describes a single video mode.
#[derive(Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct VidMode {
    pub width: u32,
    pub height: u32,
    pub red_bits: u32,
    pub green_bits: u32,
    pub blue_bits: u32,
    pub refresh_rate: u32,
}

/// Describes the gamma ramp of a monitor.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GammaRamp {
    pub red: Vec<c_ushort>,
    pub green: Vec<c_ushort>,
    pub blue: Vec<c_ushort>,
}

/// `ContextReleaseBehavior` specifies the release behavior to be used by the context.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ContextReleaseBehavior {
    Any = ffi::GLFW_ANY_RELEASE_BEHAVIOR,
    /// `Flush` tells the context to flush the pipeline whenever the context is released from being
    /// the current one.
    Flush = ffi::GLFW_RELEASE_BEHAVIOR_FLUSH,
    /// `None` tells the context to NOT flush the pipeline on release
    None = ffi::GLFW_RELEASE_BEHAVIOR_NONE,
}

/// Specifies the API to use to create the context
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ContextCreationApi {
    Native = ffi::GLFW_NATIVE_CONTEXT_API,
    Egl = ffi::GLFW_EGL_CONTEXT_API,
    OsMesa = ffi::GLFW_OSMESA_CONTEXT_API,
}

/// Specifies how the context should handle swapping the buffers.
///
/// i.e. the number of screen updates to wait from the time
/// `glfwSwapBuffers`/`context.swap_buffers`
/// was called before swapping the buffers and returning.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SwapInterval {
    /// Specifies no waits
    None,
    /// If either of the `WGL_EXT_swap_control_tear` and `GLX_EXT_swap_control_tear` extensions
    /// are enabled, allows the adaptively swap the frame. Sometimes called Adaptive V-sync
    Adaptive,
    /// Synchronizes the buffers every N frames. Set to 1 for V-sync
    Sync(u32),
}

/// An OpenGL process address.
pub type GLProc = ffi::GLFWglproc;

/// A Vulkan process address
#[cfg(feature = "vulkan")]
pub type VkProc = ffi::GLFWvkproc;

/// Counts for (Calling glfwInit) - (Calling glfwTerminate)
/// It uses for "global" refference counting for Glfw.
static REF_COUNT_FOR_GLFW: AtomicUsize = AtomicUsize::new(0);

/// A struct that represents a thread safe handle to a `Glfw`
#[derive(Debug)]
pub struct ThreadSafeGlfw {
    glfw: Glfw,
}

impl ThreadSafeGlfw {
    /// Creates a new `Glfw` wrapper that can be shared between threads
    pub fn from(glfw: &mut Glfw) -> Self {
        Self { glfw: glfw.clone() }
    }

    /// Wrapper function, please refer to [`Glfw::set_swap_interval`]
    pub fn set_swap_interval(&mut self, interval: SwapInterval) {
        self.glfw.set_swap_interval(interval);
    }

    /// Wrapper function, please refer to [`Glfw::extension_supported`]
    pub fn extension_supported(&self, extension: &str) -> bool {
        self.glfw.extension_supported(extension)
    }

    /// Wrapper function, please refer to [`Glfw::get_time`]
    pub fn get_time(&self) -> f64 {
        self.glfw.get_time()
    }

    /// Wrapper function, please refer to [`Glfw::set_time`]
    pub fn set_time(&mut self, time: f64) {
        self.glfw.set_time(time);
    }

    /// Wrapper function, please refer to [`Glfw::vulkan_supported`]
    #[cfg(feature = "vulkan")]
    pub fn vulkan_supported(&self) -> bool {
        self.glfw.vulkan_supported()
    }

    /// Wrapper function, please refer to [`Glfw::get_required_instance_extensions`]
    #[cfg(feature = "vulkan")]
    pub fn get_required_instance_extensions(&self) -> Option<Vec<String>> {
        self.glfw.get_required_instance_extensions()
    }

    /// Wrapper function, please refer to [`Glfw::get_instance_proc_address_raw`]
    #[cfg(feature = "vulkan")]
    pub fn get_instance_proc_address_raw(
        &self,
        instance: ffi::VkInstance,
        procname: &str,
    ) -> VkProc {
        self.glfw.get_instance_proc_address_raw(instance, procname)
    }

    /// Wrapper function, please refer to [`Glfw::get_physical_device_presentation_support_raw`]
    #[cfg(feature = "vulkan")]
    pub fn get_physical_device_presentation_support_raw(
        &self,
        instance: ffi::VkInstance,
        device: ffi::VkPhysicalDevice,
        queue_family: u32,
    ) -> bool {
        self.glfw
            .get_physical_device_presentation_support_raw(instance, device, queue_family)
    }

    /// Wrapper function, please refer to [`Glfw::get_timer_value`]
    pub fn get_timer_value(&self) -> u64 {
        self.glfw.get_timer_value()
    }

    /// Wrapper function, please refer to [`Glfw::get_timer_frequency`]
    pub fn get_timer_frequency(&self) -> u64 {
        self.glfw.get_timer_frequency()
    }

    /// Wrapper function, please refer to [`Glfw::post_empty_event`]
    pub fn post_empty_event(&self) {
        self.glfw.post_empty_event()
    }
}

unsafe impl Send for ThreadSafeGlfw {}

/// A token from which to call various GLFW functions. It can be obtained by
/// calling the `init` function. This cannot be sent to other tasks, and should
/// only be initialized on the main platform thread. Whilst this might make
/// performing some operations harder, this is to ensure thread safety is enforced
/// statically.
#[non_exhaustive]
#[derive(Debug)]
pub struct Glfw {
    phantom: std::marker::PhantomData<*const ()>,
}

/// An error that might be returned when `glfw::init` is called.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum InitError {
    /// Deprecated. Does not occur.
    AlreadyInitialized,
    /// An internal error occurred when trying to initialize the library.
    Internal,
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = match *self {
            InitError::AlreadyInitialized => "Already Initialized",
            InitError::Internal => "Internal Initialization Error",
        };

        f.write_str(description)
    }
}

impl error::Error for InitError {}

/// Initialization hints that can be set using the `init_hint` function.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum InitHint {
    Platform(Platform),
    /// Specifies whether to also expose joystick hats as buttons, for compatibility with earlier
    /// versions of GLFW that did not have `glfwGetJoystickHats`.
    JoystickHatButtons(bool),
    /// Specifies whether to set the current directory to the application to the
    /// `Contents/Resources` subdirectory of the application's bundle, if present.
    ///
    /// This is ignored on platforms besides macOS.
    CocoaChdirResources(bool),
    /// Specifies whether to create a basic menu bar, either from a nib or manually, when the first
    /// window is created, which is when AppKit is initialized.
    ///
    /// This is ignored on platforms besides macOS.
    CocoaMenubar(bool),
}

/// The platform to use when initializing GLFW.
/// see [InitHint::Platform]
///
/// To check if a particular platform is supported, use [`Platform::is_supported`]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(i32)]
pub enum Platform {
    X11 = ffi::GLFW_PLATFORM_X11,
    Wayland = ffi::GLFW_PLATFORM_WAYLAND,
    Win32 = ffi::GLFW_PLATFORM_WIN32,
    MacOS = ffi::GLFW_PLATFORM_COCOA,
    /// Useful for testing.
    Null = ffi::GLFW_PLATFORM_NULL,
    /// Chooses the best available platform.
    Any = ffi::GLFW_ANY_PLATFORM,
}
impl Platform {
    /// Whether this platform is supported.
    pub fn is_supported(&self) -> bool {
        unsafe { ffi::glfwPlatformSupported(*self as c_int) == ffi::GLFW_TRUE }
    }
}
/// Sets hints for the next initialization of GLFW.
///
/// The values you set hints to are never reset by GLFW, but they only take effect during
/// initialization. Once GLFW has been initialized, any values you set will be ignored until the
/// library is terminated and initialized again.
///
/// Wrapper for `glfwInitHint`.
pub fn init_hint(hint: InitHint) {
    match hint {
        InitHint::Platform(platform) => unsafe {
            ffi::glfwInitHint(ffi::GLFW_PLATFORM, platform as c_int)
        },
        InitHint::JoystickHatButtons(joystick_hat_buttons) => unsafe {
            ffi::glfwInitHint(
                ffi::GLFW_JOYSTICK_HAT_BUTTONS,
                joystick_hat_buttons as c_int,
            )
        },
        InitHint::CocoaChdirResources(chdir) => unsafe {
            ffi::glfwInitHint(ffi::GLFW_COCOA_CHDIR_RESOURCES, chdir as c_int)
        },
        InitHint::CocoaMenubar(menubar) => unsafe {
            ffi::glfwInitHint(ffi::GLFW_COCOA_MENUBAR, menubar as c_int)
        },
    }
}
/// Initializes the GLFW library. This must be called on the main platform
/// thread.
///
/// Wrapper for `glfwInit`.
///
/// # Example
///
/// ~~~no_run
/// extern crate glfw;
///
/// fn main() {
///    let glfw = glfw::init_no_callbacks().unwrap();
/// }
/// ~~~
///
/// # Error callback
///
/// An error callback can be set if desired. This allows for the handling of any
/// errors that occur during initialization. This can subsequently be changed
/// using the `glfw::init` function.
///
/// ~~~no_run
/// extern crate glfw;
/// #[macro_use]
/// extern crate log;
///
/// fn main() {
///    let glfw = glfw::init(error_callback).unwrap();
/// }
///
/// fn error_callback(err: glfw::Error, description: String) {
///     error!("GLFW error {:?}: {:?}", err, description);
/// }
/// ~~~
///
/// # Returns
///
/// - If initialization was successful a `Glfw` token will be returned along with a `Receiver` from
///   which errors can be intercepted.
/// - Subsequent calls to `init` will return `Glfw` token immediately.
/// - If an initialization error occurred within the GLFW library `Err(InternalInitError)` will be
///   returned.
pub fn init<T>(callback: T) -> Result<Glfw, InitError>
where
    T: FnMut(Error, String) + 'static,
{
    // Initialize the error callback. This is done
    // before `ffi::glfwInit` because errors could occur during
    // initialization.
    callbacks::error::set(callback);

    init_no_callbacks()
}

pub fn init_no_callbacks() -> Result<Glfw, InitError> {
    // initialize GLFW.
    // FYI: multiple not terminated ffi::glfwInit() returns ffi::GLFW_TRUE immediately.
    // https://www.glfw.org/docs/latest/group__init.html#ga317aac130a235ab08c6db0834907d85e
    if unsafe { ffi::glfwInit() } == ffi::GLFW_TRUE {
        REF_COUNT_FOR_GLFW.fetch_add(1, Ordering::SeqCst);
        Ok(Glfw {
            phantom: std::marker::PhantomData,
        })
    } else {
        Err(InitError::Internal)
    }
}

impl Glfw {
    /// Sets the error callback, overwriting the previous one stored.
    ///
    /// # Example
    ///
    /// ~~~ignore
    /// // sets a new callback
    /// let mut error_count: usize = 0;
    /// glfw.set_error_callback(Some(move |error, description| {
    ///     println!("GLFW error {}: {}", error_count, description);
    ///     error_count += 1;
    /// }));
    ///
    /// // removes the previously set callback
    /// glfw.set_error_callback(None);
    /// ~~~
    ///
    /// The `fail_on_errors!()` and `log_errors!()` callback macros are provided for
    /// convenience. For example:
    ///
    /// ~~~ignore
    /// // triggers a task failure when a GLFW error is encountered.
    /// glfw.set_error_callback(fail_on_errors!());
    /// ~~~
    pub fn set_error_callback<T>(&mut self, callback: T)
    where
        T: FnMut(Error, String) + 'static,
    {
        callbacks::error::set(callback);
    }

    /// Unsets the monitor callback
    pub fn unset_error_callback(&mut self) {
        callbacks::error::unset();
    }

    /// Sets the monitor callback, overwriting the previous one stored.
    pub fn set_monitor_callback<T>(&mut self, callback: T)
    where
        T: FnMut(Monitor, MonitorEvent) + 'static,
    {
        callbacks::monitor::set(callback);
    }

    /// Unsets the monitor callback
    pub fn unset_monitor_callback(&mut self) {
        callbacks::monitor::unset();
    }

    /// Sets the joystick callback, overwriting the previous one stored
    pub fn set_joystick_callback<T>(&mut self, callback: T)
    where
        T: FnMut(JoystickId, JoystickEvent) + 'static,
    {
        callbacks::joystick::set(callback);
    }

    /// Unsets the joystick callback
    pub fn unset_joystick_callback(&mut self) {
        callbacks::joystick::unset();
    }

    /// Supplies the primary monitor to the closure provided, if it exists.
    /// This is usually the monitor where elements like the Windows task bar or
    /// the OS X menu bar is located.
    ///
    /// # Example
    ///
    /// ~~~ignore
    /// let (window, events) = glfw.with_primary_monitor(|_, m| {
    ///     glfw.create_window(300, 300, "Hello this is window",
    ///         m.map_or(glfw::WindowMode::Windowed, |m| glfw::FullScreen(m)))
    /// }).expect("Failed to create GLFW window.");
    /// ~~~
    pub fn with_primary_monitor<T, F>(&mut self, f: F) -> T
    where
        F: FnOnce(&mut Self, Option<&mut Monitor>) -> T,
    {
        match unsafe { ffi::glfwGetPrimaryMonitor() } {
            ptr if ptr.is_null() => f(self, None),
            ptr => f(self, Some(&mut Monitor { ptr })),
        }
    }

    /// Supplies the window monitor to the closure provided, if it's fullscreen.
    ///
    /// # Example
    ///
    /// ~~~ignore
    /// let (window, events) = glfw.with_window_monitor(|_, m| {
    ///     glfw.create_window(300, 300, "Hello this is window",
    ///         m.map_or(glfw::WindowMode::Windowed, |m| glfw::FullScreen(m)))
    /// }).expect("Failed to create GLFW window.");
    /// ~~~
    pub fn with_window_monitor<T, F>(&mut self, window: &mut Window, f: F) -> T
    where
        F: FnOnce(&mut Self, Option<&mut Monitor>) -> T,
    {
        match unsafe { ffi::glfwGetWindowMonitor(window.ptr) } {
            ptr if ptr.is_null() => f(self, None),
            ptr => f(self, Some(&mut Monitor { ptr })),
        }
    }

    /// Supplies a vector of the currently connected monitors to the closure
    /// provided.
    ///
    /// # Example
    ///
    /// ~~~ignore
    /// glfw.with_connected_monitors(|_, monitors| {
    ///     for monitor in monitors.iter() {
    ///         println!("{}: {}", monitor.get_name(), monitor.get_video_mode());
    ///     }
    /// });
    /// ~~~
    pub fn with_connected_monitors<T, F>(&mut self, f: F) -> T
    where
        F: FnOnce(&mut Self, &[&mut Monitor]) -> T,
    {
        unsafe {
            let mut count = 0;
            let ptr = ffi::glfwGetMonitors(&mut count);
            let mut monitors;
            let refs: Vec<&mut Monitor> = if ptr.is_null() {
                Vec::new()
            } else {
                monitors = slice::from_raw_parts(ptr as *const _, count as usize)
                    .iter()
                    .map(|&ptr| Monitor { ptr })
                    .collect::<Vec<Monitor>>();
                monitors.iter_mut().collect()
            };
            f(self, &refs)
        }
    }

    /// Queries Vulkan support via `glfwVulkanSupported`
    #[cfg(feature = "vulkan")]
    pub fn vulkan_supported(&self) -> bool {
        unsafe { ffi::glfwVulkanSupported() == ffi::GLFW_TRUE }
    }

    /// This is used to set the window hints for the next call to
    /// `Glfw::create_window`. The hints can be reset to their default values
    /// using calling the `Glfw::default_window_hints` function.
    ///
    /// Wrapper for `glfwWindowHint`
    ///
    /// # OpenGL 3.x and 4.x on Mac OS X
    ///
    /// The only OpenGL 3.x and 4.x contexts supported by OS X are
    /// forward-compatible, core profile contexts.
    ///
    /// 10.7 and 10.8 support the following OpenGL versions:
    ///
    /// - `glfw::WindowHint::ContextVersion(3, 2)`
    ///
    /// 10.9 supports the following OpenGL versions
    ///
    /// - `glfw::WindowHint::ContextVersion(3, 2)`
    /// - `glfw::WindowHint::ContextVersion(3, 3)`
    /// - `glfw::WindowHint::ContextVersion(4, 1)`
    ///
    /// To create an OS X compatible context, the hints should be specified as
    /// follows:
    ///
    /// ~~~ignore
    /// glfw.window_hint(glfw::WindowHint::ContextVersion(3, 2));
    /// glfw.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));
    /// glfw.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));
    /// ~~~
    pub fn window_hint(&mut self, hint: WindowHint) {
        //This is just a simple function to unwrap the option and convert it to `c_int` or use
        // `GLFW_DONT_CARE`, then call `glfwWindowHint` with the result. It was required because
        // `GLFW_DONT_CARE` is signed, so `value.unwrap_or(ffi::GLFW_DONT_CARE)` wouldn't work because
        // of the type difference.
        #[inline(always)]
        unsafe fn dont_care_hint(hint: c_int, value: Option<u32>) {
            ffi::glfwWindowHint(hint, unwrap_dont_care(value))
        }

        #[inline(always)]
        unsafe fn string_hint(hint: c_int, value: Option<String>) {
            let value = if let Some(value) = &value {
                value.as_str()
            } else {
                ""
            };
            with_c_str(value, |value| ffi::glfwWindowHintString(hint, value))
        }

        match hint {
            WindowHint::MousePassthrough(value) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_MOUSE_PASSTHROUGH, value as c_int)
            },
            WindowHint::RedBits(bits) => unsafe { dont_care_hint(ffi::GLFW_RED_BITS, bits) },
            WindowHint::GreenBits(bits) => unsafe { dont_care_hint(ffi::GLFW_GREEN_BITS, bits) },
            WindowHint::BlueBits(bits) => unsafe { dont_care_hint(ffi::GLFW_BLUE_BITS, bits) },
            WindowHint::AlphaBits(bits) => unsafe { dont_care_hint(ffi::GLFW_ALPHA_BITS, bits) },
            WindowHint::DepthBits(bits) => unsafe { dont_care_hint(ffi::GLFW_DEPTH_BITS, bits) },
            WindowHint::StencilBits(bits) => unsafe {
                dont_care_hint(ffi::GLFW_STENCIL_BITS, bits)
            },
            WindowHint::AccumRedBits(bits) => unsafe {
                dont_care_hint(ffi::GLFW_ACCUM_RED_BITS, bits)
            },
            WindowHint::AccumGreenBits(bits) => unsafe {
                dont_care_hint(ffi::GLFW_ACCUM_GREEN_BITS, bits)
            },
            WindowHint::AccumBlueBits(bits) => unsafe {
                dont_care_hint(ffi::GLFW_ACCUM_BLUE_BITS, bits)
            },
            WindowHint::AccumAlphaBits(bits) => unsafe {
                dont_care_hint(ffi::GLFW_ACCUM_ALPHA_BITS, bits)
            },
            WindowHint::AuxBuffers(num_buffers) => unsafe {
                dont_care_hint(ffi::GLFW_AUX_BUFFERS, num_buffers)
            },
            WindowHint::Samples(num_samples) => unsafe {
                dont_care_hint(ffi::GLFW_SAMPLES, num_samples)
            },
            WindowHint::RefreshRate(rate) => unsafe {
                dont_care_hint(ffi::GLFW_REFRESH_RATE, rate)
            },
            WindowHint::Stereo(is_stereo) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_STEREO, is_stereo as c_int)
            },
            WindowHint::SRgbCapable(is_capable) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_SRGB_CAPABLE, is_capable as c_int)
            },
            WindowHint::ClientApi(api) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_CLIENT_API, api as c_int)
            },
            WindowHint::ContextVersionMajor(major) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_CONTEXT_VERSION_MAJOR, major as c_int)
            },
            WindowHint::ContextVersionMinor(minor) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_CONTEXT_VERSION_MINOR, minor as c_int)
            },
            WindowHint::ContextVersion(major, minor) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_CONTEXT_VERSION_MAJOR, major as c_int);
                ffi::glfwWindowHint(ffi::GLFW_CONTEXT_VERSION_MINOR, minor as c_int)
            },
            WindowHint::ContextRobustness(robustness) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_CONTEXT_ROBUSTNESS, robustness as c_int)
            },
            WindowHint::OpenGlForwardCompat(is_compat) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_OPENGL_FORWARD_COMPAT, is_compat as c_int)
            },
            WindowHint::OpenGlDebugContext(is_debug) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_OPENGL_DEBUG_CONTEXT, is_debug as c_int)
            },
            WindowHint::OpenGlProfile(profile) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_OPENGL_PROFILE, profile as c_int)
            },
            WindowHint::Resizable(is_resizable) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_RESIZABLE, is_resizable as c_int)
            },
            WindowHint::Visible(is_visible) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_VISIBLE, is_visible as c_int)
            },
            WindowHint::Decorated(is_decorated) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_DECORATED, is_decorated as c_int)
            },
            WindowHint::AutoIconify(auto_iconify) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_AUTO_ICONIFY, auto_iconify as c_int)
            },
            WindowHint::Floating(is_floating) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_FLOATING, is_floating as c_int)
            },
            WindowHint::Focused(is_focused) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_FOCUSED, is_focused as c_int)
            },
            WindowHint::Maximized(is_maximized) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_MAXIMIZED, is_maximized as c_int)
            },
            WindowHint::ContextNoError(is_no_error) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_CONTEXT_NO_ERROR, is_no_error as c_int)
            },
            WindowHint::ContextCreationApi(api) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_CONTEXT_CREATION_API, api as c_int)
            },
            WindowHint::ContextReleaseBehavior(behavior) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_CONTEXT_RELEASE_BEHAVIOR, behavior as c_int)
            },
            WindowHint::DoubleBuffer(is_dbuffered) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_DOUBLEBUFFER, is_dbuffered as c_int)
            },
            WindowHint::CenterCursor(center_cursor) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_CENTER_CURSOR, center_cursor as c_int)
            },
            WindowHint::TransparentFramebuffer(is_transparent) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_TRANSPARENT_FRAMEBUFFER, is_transparent as c_int)
            },
            WindowHint::FocusOnShow(focus) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_FOCUS_ON_SHOW, focus as c_int)
            },
            WindowHint::ScaleToMonitor(scale) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_SCALE_TO_MONITOR, scale as c_int)
            },
            WindowHint::CocoaRetinaFramebuffer(retina_fb) => unsafe {
                ffi::glfwWindowHint(ffi::GLFW_COCOA_RETINA_FRAMEBUFFER, retina_fb as c_int)
            },
            WindowHint::CocoaFrameName(name) => unsafe {
                string_hint(ffi::GLFW_COCOA_FRAME_NAME, name)
            },
            WindowHint::CocoaGraphicsSwitching(graphics_switching) => unsafe {
                ffi::glfwWindowHint(
                    ffi::GLFW_COCOA_GRAPHICS_SWITCHING,
                    graphics_switching as c_int,
                )
            },
            WindowHint::X11ClassName(class_name) => unsafe {
                string_hint(ffi::GLFW_X11_CLASS_NAME, class_name)
            },
            WindowHint::X11InstanceName(instance_name) => unsafe {
                string_hint(ffi::GLFW_X11_INSTANCE_NAME, instance_name)
            },
        }
    }

    /// Resets the window hints previously set by the `window_hint` function to
    /// their default values.
    ///
    /// Wrapper for `glfwDefaultWindowHints`.
    pub fn default_window_hints(&mut self) {
        unsafe {
            ffi::glfwDefaultWindowHints();
        }
    }

    /// Creates a new window.
    ///
    /// Wrapper for `glfwCreateWindow`.
    pub fn create_window(
        &mut self,
        width: u32,
        height: u32,
        title: &str,
        mode: WindowMode<'_>,
    ) -> Option<(PWindow, GlfwReceiver<(f64, WindowEvent)>)> {
        #[cfg(feature = "wayland")]
        {
            // Has to be set otherwise wayland refuses to open window.
            self.window_hint(WindowHint::Focused(false));
        }
        self.create_window_intern(width, height, title, mode, None)
    }

    /// Internal wrapper for `glfwCreateWindow`.
    fn create_window_intern(
        &self,
        width: u32,
        height: u32,
        title: &str,
        mode: WindowMode<'_>,
        share: Option<&Window>,
    ) -> Option<(PWindow, GlfwReceiver<(f64, WindowEvent)>)> {
        let ptr = unsafe {
            with_c_str(title, |title| {
                ffi::glfwCreateWindow(
                    width as c_int,
                    height as c_int,
                    title,
                    mode.to_ptr(),
                    match share {
                        Some(w) => w.ptr,
                        None => ptr::null_mut(),
                    },
                )
            })
        };
        if ptr.is_null() {
            None
        } else {
            let (drop_sender, drop_receiver) = channel();
            let (sender, receiver) = glfw_channel(16, 256);
            let window = Window {
                ptr,
                glfw: self.clone(),
                is_shared: share.is_some(),
                drop_sender: Some(drop_sender),
                drop_receiver,
                current_cursor: None,
            };
            let mut callbacks = Box::new(WindowCallbacks::new(sender));
            let mut window = PWindow(Box::new(window));

            unsafe {
                callbacks.window_ptr = window.raw_ptr();
                ffi::glfwSetWindowUserPointer(ptr, mem::transmute(callbacks));
            }

            Some((window, receiver))
        }
    }

    /// Makes the context of the specified window current. If no window is given
    /// then the current context is detached.
    ///
    /// Wrapper for `glfwMakeContextCurrent`.
    pub fn make_context_current(&mut self, context: Option<&Window>) {
        match context {
            Some(window) => unsafe { ffi::glfwMakeContextCurrent(window.ptr) },
            None => unsafe { ffi::glfwMakeContextCurrent(ptr::null_mut()) },
        }
    }

    /// Wrapper for `glfwGetX11Display`
    #[cfg(all(
        not(target_os = "macos"),
        not(target_os = "windows"),
        not(target_os = "emscripten"),
        feature = "x11",
        feature = "native-handles"
    ))]
    pub fn get_x11_display(&self) -> *mut c_void {
        unsafe { ffi::glfwGetX11Display() }
    }

    /// Wrapper for `glfwGetWaylandDisplay`
    #[cfg(all(
        not(target_os = "macos"),
        not(target_os = "windows"),
        not(target_os = "emscripten"),
        feature = "wayland",
        feature = "native-handles"
    ))]
    pub fn get_wayland_display(&self) -> *mut c_void {
        unsafe { ffi::glfwGetWaylandDisplay().cast_mut() }
    }
    /// Wrapper for `glfwGetPlatform`
    pub fn get_platform(&self) -> Platform {
        unsafe { mem::transmute(ffi::glfwGetPlatform()) }
    }
    /// Immediately process the received events.
    ///
    /// Wrapper for `glfwPollEvents`.
    pub fn poll_events(&mut self) {
        unsafe {
            ffi::glfwPollEvents();
        }
    }

    /// Immediately process the received events. The *unbuffered* variant differs by allowing
    /// inspection of events *prior* to their associated native callback returning. This also
    /// provides a way to synchronously respond to the event. Events returned by the closure
    /// are delivered to the channel receiver just as if `poll_events` was called. Returning
    /// `None` from the closure will drop the event.
    ///
    /// Wrapper for `glfwPollEvents`.
    pub fn poll_events_unbuffered<F>(&mut self, mut f: F)
    where
        F: FnMut(WindowId, (f64, WindowEvent)) -> Option<(f64, WindowEvent)>,
    {
        let _unset_handler_guard = unsafe { crate::callbacks::unbuffered::set_handler(&mut f) };
        self.poll_events();
    }

    /// Sleep until at least one event has been received, and then perform the
    /// equivalent of `Glfw::poll_events`.
    ///
    /// Wrapper for `glfwWaitEvents`.
    pub fn wait_events(&mut self) {
        unsafe {
            ffi::glfwWaitEvents();
        }
    }

    /// Sleep until at least one event has been received, and then perform the
    /// equivalent of `Glfw::poll_events_unbuffered`.
    ///
    /// Wrapper for `glfwWaitEvents`.
    pub fn wait_events_unbuffered<F>(&mut self, mut f: F)
    where
        F: FnMut(WindowId, (f64, WindowEvent)) -> Option<(f64, WindowEvent)>,
    {
        let _unset_handler_guard = unsafe { crate::callbacks::unbuffered::set_handler(&mut f) };
        self.wait_events();
    }

    /// Sleep until at least one event has been received, or until the specified
    /// timeout is reached, and then perform the equivalent of `Glfw::poll_events`.
    /// Timeout is specified in seconds.
    ///
    /// Wrapper for `glfwWaitEventsTimeout`.
    pub fn wait_events_timeout(&mut self, timeout: f64) {
        unsafe {
            ffi::glfwWaitEventsTimeout(timeout);
        }
    }

    /// Sleep until at least one event has been received, or until the specified
    /// timeout is reached, and then perform the equivalent of `Glfw::poll_events_unbuffered`.
    /// Timeout is specified in seconds.
    ///
    /// Wrapper for `glfwWaitEventsTimeout`.
    pub fn wait_events_timeout_unbuffered<F>(&mut self, timeout: f64, mut f: F)
    where
        F: FnMut(WindowId, (f64, WindowEvent)) -> Option<(f64, WindowEvent)>,
    {
        let _unset_handler_guard = unsafe { crate::callbacks::unbuffered::set_handler(&mut f) };
        self.wait_events_timeout(timeout);
    }

    /// Posts an empty event from the current thread to the event queue, causing
    /// `wait_events` or `wait_events_timeout` to return.
    /// If no windows exist, this function returns immediately.
    ///
    /// Wrapper for `glfwPostEmptyEvent`.
    pub fn post_empty_event(&self) {
        unsafe {
            ffi::glfwPostEmptyEvent();
        }
    }

    /// Returns the current value of the GLFW timer. Unless the timer has been
    /// set using `glfw::set_time`, the timer measures time elapsed since GLFW
    /// was initialized.
    ///
    /// Wrapper for `glfwGetTime`.
    pub fn get_time(&self) -> f64 {
        unsafe { ffi::glfwGetTime() as f64 }
    }

    /// Sets the value of the GLFW timer.
    ///
    /// Wrapper for `glfwSetTime`.
    pub fn set_time(&mut self, time: f64) {
        unsafe {
            ffi::glfwSetTime(time as c_double);
        }
    }

    /// Wrapper for `glfwGetTimerValue`.
    pub fn get_timer_value(&self) -> u64 {
        unsafe { ffi::glfwGetTimerValue() as u64 }
    }

    /// Wrapper for `glfwGetTimerFrequency`
    pub fn get_timer_frequency(&self) -> u64 {
        unsafe { ffi::glfwGetTimerFrequency() as u64 }
    }

    /// Sets the number of screen updates to wait before swapping the buffers of
    /// the current context and returning from `Window::swap_buffers`.
    ///
    /// Wrapper for `glfwSwapInterval`.
    pub fn set_swap_interval(&mut self, interval: SwapInterval) {
        unsafe {
            ffi::glfwSwapInterval(match interval {
                SwapInterval::None => 0_i32,
                SwapInterval::Adaptive => -1_i32,
                SwapInterval::Sync(interval) => interval as c_int,
            })
        }
    }

    /// Returns `true` if the specified OpenGL or context creation API extension
    /// is supported by the current context.
    ///
    /// Wrapper for `glfwExtensionSupported`.
    pub fn extension_supported(&self, extension: &str) -> bool {
        unsafe {
            with_c_str(extension, |extension| {
                ffi::glfwExtensionSupported(extension) == ffi::GLFW_TRUE
            })
        }
    }

    /// Wrapper for `glfwGetRequiredInstanceExtensions`
    ///
    /// This function returns a Vector of names of Vulkan instance extensions
    /// required by GLFW for creating Vulkan surfaces for GLFW windows. If successful,
    /// the list will always contains `VK_KHR_surface`, so if you don't require any
    /// additional extensions you can pass this list directly to the `VkInstanceCreateInfo` struct.
    ///
    /// Will return `None` if the API is unavailable.
    #[cfg(feature = "vulkan")]
    pub fn get_required_instance_extensions(&self) -> Option<Vec<String>> {
        let mut len: c_uint = 0;

        unsafe {
            let raw_extensions: *const *const c_char =
                ffi::glfwGetRequiredInstanceExtensions(&mut len as *mut c_uint);

            if !raw_extensions.is_null() {
                return Some(
                    slice::from_raw_parts(raw_extensions, len as usize)
                        .iter()
                        .map(|extensions| string_from_c_str(*extensions))
                        .collect(),
                );
            }
        }

        None
    }

    /// Returns the address of the specified client API or extension function if
    /// it is supported by the current context, NULL otherwise.
    ///
    /// Wrapper for `glfwGetProcAddress`.
    pub fn get_proc_address_raw(&self, procname: &str) -> GLProc {
        debug_assert!(unsafe { ffi::glfwGetCurrentContext() } != std::ptr::null_mut());
        with_c_str(procname, |procname| unsafe {
            ffi::glfwGetProcAddress(procname)
        })
    }

    /// This function returns the address of the specified Vulkan core or extension function
    /// for the specified instance. If instance is set to NULL it can return any function
    /// exported from the Vulkan loader, including at least the following functions:
    ///
    /// * `vkEnumerateInstanceExtensionProperties`
    /// * `vkEnumerateInstanceLayerProperties`
    /// * `vkCreateInstance`
    /// * `vkGetInstanceProcAddr`
    ///
    /// If Vulkan is not available on the machine, this function returns `NULL`
    ///
    /// Wrapper for `glfwGetInstanceProcAddress`
    #[cfg(feature = "vulkan")]
    pub fn get_instance_proc_address_raw(
        &self,
        instance: ffi::VkInstance,
        procname: &str,
    ) -> VkProc {
        with_c_str(procname, |procname| unsafe {
            ffi::glfwGetInstanceProcAddress(instance, procname)
        })
    }

    /// This function returns whether the specified queue family of the specified
    /// physical device supports presentation to the platform GLFW was built for.
    ///
    /// Wrapper for `glfwGetPhysicalDevicePresentationSupport`
    #[cfg(feature = "vulkan")]
    pub fn get_physical_device_presentation_support_raw(
        &self,
        instance: ffi::VkInstance,
        device: ffi::VkPhysicalDevice,
        queue_family: u32,
    ) -> bool {
        ffi::GLFW_TRUE
            == unsafe {
                ffi::glfwGetPhysicalDevicePresentationSupport(
                    instance,
                    device,
                    queue_family as c_uint,
                )
            }
    }

    /// Constructs a `Joystick` handle corresponding to the supplied `JoystickId`.
    pub fn get_joystick(&self, id: JoystickId) -> Joystick {
        Joystick {
            id,
            glfw: self.clone(),
        }
    }

    /// Wrapper for `glfwRawMouseMotionSupported`.
    pub fn supports_raw_motion(&self) -> bool {
        unsafe { ffi::glfwRawMouseMotionSupported() == ffi::GLFW_TRUE }
    }

    /// Parses the specified ASCII encoded string and updates the internal list with any gamepad
    /// mappings it finds. This string may contain either a single gamepad mapping or many mappings
    /// separated by newlines. The parser supports the full format of the `gamecontrollerdb.txt`
    /// source file including empty lines and comments.
    ///
    /// Wrapper for `glfwUpdateGamepadMappings`.
    ///
    /// # Returns
    ///
    /// `true` if successful, or `false` if an error occurred.
    pub fn update_gamepad_mappings(&self, mappings: &str) -> bool {
        unsafe {
            with_c_str(mappings, |mappings| {
                ffi::glfwUpdateGamepadMappings(mappings) == ffi::GLFW_TRUE
            })
        }
    }
}

impl Clone for Glfw {
    fn clone(&self) -> Self {
        REF_COUNT_FOR_GLFW.fetch_add(1, Ordering::SeqCst);
        Glfw {
            phantom: std::marker::PhantomData,
        }
    }
}

impl Drop for Glfw {
    fn drop(&mut self) {
        let old_diff = REF_COUNT_FOR_GLFW.fetch_sub(1, Ordering::SeqCst);
        if old_diff == 1 {
            unsafe {
                ffi::glfwTerminate();
            }
        }
    }
}

fn glfw_channel<T>(initial_capacity: usize, max_len: usize) -> (GlfwSender<T>, GlfwReceiver<T>) {
    let shared = Arc::new(SharedTransmitter {
        queue: Mutex::new(VecDeque::with_capacity(initial_capacity)),
        max_len,
    });
    let (mpsc_sender, mpsc_receiver) = channel();

    let sender = GlfwSender {
        transmitter: shared.clone(),
        sender: mpsc_sender,
    };
    let receiver = GlfwReceiver {
        transmitter: shared.clone(),
        receiver: mpsc_receiver,
    };
    (sender, receiver)
}

#[derive(Debug)]
struct SharedTransmitter<T> {
    queue: Mutex<VecDeque<T>>,
    max_len: usize,
}

#[derive(Debug, Clone)]
struct GlfwSender<T> {
    transmitter: Arc<SharedTransmitter<T>>,
    sender: Sender<T>,
}

impl<T> GlfwSender<T> {
    fn send(&self, v: T) {
        let mut queue = self.transmitter.queue.lock().unwrap();
        if queue.len() >= self.transmitter.max_len {
            let _ = self.sender.send(v);
        } else {
            queue.push_back(v);
        }
    }
}

#[derive(Debug)]
pub struct GlfwReceiver<T> {
    transmitter: Arc<SharedTransmitter<T>>,
    receiver: Receiver<T>,
}

impl<T> GlfwReceiver<T> {
    pub fn receive(&self) -> Option<T> {
        let ret = self.transmitter.queue.lock().unwrap().pop_front();
        if ret.is_some() {
            ret
        } else {
            match self.receiver.try_recv() {
                Ok(ret) => Some(ret),
                Err(_) => None,
            }
        }
    }
}

struct WindowCallbacks {
    window_ptr: *mut Window,
    sender: GlfwSender<(f64, WindowEvent)>,
    pos_callback: Option<Box<dyn FnMut(&mut Window, i32, i32)>>,
    size_callback: Option<Box<dyn FnMut(&mut Window, i32, i32)>>,
    close_callback: Option<Box<dyn FnMut(&mut Window)>>,
    refresh_callback: Option<Box<dyn FnMut(&mut Window)>>,
    focus_callback: Option<Box<dyn FnMut(&mut Window, bool)>>,
    iconify_callback: Option<Box<dyn FnMut(&mut Window, bool)>>,
    framebuffer_size_callback: Option<Box<dyn FnMut(&mut Window, i32, i32)>>,
    key_callback: Option<Box<dyn FnMut(&mut Window, Key, Scancode, Action, Modifiers)>>,
    char_callback: Option<Box<dyn FnMut(&mut Window, char)>>,
    char_mods_callback: Option<Box<dyn FnMut(&mut Window, char, Modifiers)>>,
    mouse_button_callback: Option<Box<dyn FnMut(&mut Window, MouseButton, Action, Modifiers)>>,
    cursor_pos_callback: Option<Box<dyn FnMut(&mut Window, f64, f64)>>,
    cursor_enter_callback: Option<Box<dyn FnMut(&mut Window, bool)>>,
    scroll_callback: Option<Box<dyn FnMut(&mut Window, f64, f64)>>,
    drag_and_drop_callback: Option<Box<dyn FnMut(&mut Window, Vec<PathBuf>)>>,
    maximize_callback: Option<Box<dyn FnMut(&mut Window, bool)>>,
    content_scale_callback: Option<Box<dyn FnMut(&mut Window, f32, f32)>>,
    pos_polling: bool,
    size_polling: bool,
    close_polling: bool,
    refresh_polling: bool,
    focus_polling: bool,
    iconify_polling: bool,
    framebuffer_size_polling: bool,
    key_polling: bool,
    char_polling: bool,
    char_mods_polling: bool,
    mouse_button_polling: bool,
    cursor_pos_polling: bool,
    cursor_enter_polling: bool,
    scroll_polling: bool,
    drag_and_drop_polling: bool,
    maximize_polling: bool,
    content_scale_polling: bool,
}

impl WindowCallbacks {
    fn new(sender: GlfwSender<(f64, WindowEvent)>) -> Self {
        Self {
            window_ptr: std::ptr::null_mut(),
            sender,
            pos_callback: None,
            size_callback: None,
            close_callback: None,
            refresh_callback: None,
            focus_callback: None,
            iconify_callback: None,
            framebuffer_size_callback: None,
            key_callback: None,
            char_callback: None,
            char_mods_callback: None,
            mouse_button_callback: None,
            cursor_pos_callback: None,
            cursor_enter_callback: None,
            scroll_callback: None,
            drag_and_drop_callback: None,
            maximize_callback: None,
            content_scale_callback: None,
            pos_polling: false,
            size_polling: false,
            close_polling: false,
            refresh_polling: false,
            focus_polling: false,
            iconify_polling: false,
            framebuffer_size_polling: false,
            key_polling: false,
            char_polling: false,
            char_mods_polling: false,
            mouse_button_polling: false,
            cursor_pos_polling: false,
            cursor_enter_polling: false,
            scroll_polling: false,
            drag_and_drop_polling: false,
            maximize_polling: false,
            content_scale_polling: false,
        }
    }

    fn get_callbacks<'a>(window: *mut GLFWwindow) -> &'a mut WindowCallbacks {
        unsafe { &mut *(ffi::glfwGetWindowUserPointer(window) as *mut WindowCallbacks) }
    }
}

/// Wrapper for `glfwGetError`.
pub fn get_error() -> Error {
    Error::from_raw(unsafe { ffi::glfwGetError(null_mut()) })
}

/// Wrapper for `glfwGetError`.
pub fn get_error_string() -> (Error, String) {
    unsafe {
        let mut description: *const c_char = null();
        let error = Error::from_raw(ffi::glfwGetError(&mut description));
        let description = if description.is_null() {
            String::new()
        } else {
            string_from_c_str(description)
        };
        (error, description)
    }
}

/// Wrapper for `glfwGetVersion`.
pub fn get_version() -> Version {
    unsafe {
        let mut major = 0;
        let mut minor = 0;
        let mut patch = 0;
        ffi::glfwGetVersion(&mut major, &mut minor, &mut patch);
        Version {
            major: major as u64,
            minor: minor as u64,
            patch: patch as u64,
        }
    }
}

/// Replacement for `String::from_raw_buf`
pub unsafe fn string_from_c_str(c_str: *const c_char) -> String {
    String::from_utf8_lossy(CStr::from_ptr(c_str).to_bytes()).into_owned()
}

/// Like `string_from_c_str`, but handles null pointers correctly
pub unsafe fn string_from_nullable_c_str(c_str: *const c_char) -> Option<String> {
    if c_str.is_null() {
        None
    } else {
        Some(string_from_c_str(c_str))
    }
}

/// Replacement for `ToCStr::with_c_str`
pub fn with_c_str<F, T>(s: &str, f: F) -> T
where
    F: FnOnce(*const c_char) -> T,
{
    let c_str = CString::new(s.as_bytes());
    f(c_str.unwrap().as_bytes_with_nul().as_ptr() as *const _)
}

/// Wrapper for `glfwGetVersionString`.
pub fn get_version_string() -> String {
    unsafe { string_from_c_str(ffi::glfwGetVersionString()) }
}

/// A struct that wraps a `*GLFWmonitor` handle.
#[allow(missing_copy_implementations)]
pub struct Monitor {
    ptr: *mut ffi::GLFWmonitor,
}

impl std::fmt::Debug for Monitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Monitor({:p})", self.ptr)
    }
}

impl Monitor {
    /// Wrapper for `glfwGetMonitorPos`.
    pub fn get_pos(&self) -> (i32, i32) {
        unsafe {
            let mut xpos = 0;
            let mut ypos = 0;
            ffi::glfwGetMonitorPos(self.ptr, &mut xpos, &mut ypos);
            (xpos as i32, ypos as i32)
        }
    }

    /// Wrapper for `glfwGetMonitorPhysicalSize`.
    pub fn get_physical_size(&self) -> (i32, i32) {
        unsafe {
            let mut width = 0;
            let mut height = 0;
            ffi::glfwGetMonitorPhysicalSize(self.ptr, &mut width, &mut height);
            (width as i32, height as i32)
        }
    }

    /// Wrapper for `glfwGetMonitorName`.
    pub fn get_name(&self) -> Option<String> {
        unsafe { string_from_nullable_c_str(ffi::glfwGetMonitorName(self.ptr)) }
    }

    /// Wrapper for `glfwGetVideoModes`.
    pub fn get_video_modes(&self) -> Vec<VidMode> {
        unsafe {
            let mut count = 0;
            let ptr = ffi::glfwGetVideoModes(self.ptr, &mut count);
            if ptr.is_null() {
                return Vec::new();
            }
            slice::from_raw_parts(ptr, count as usize)
                .iter()
                .map(VidMode::from_glfw_vid_mode)
                .collect()
        }
    }

    /// Wrapper for `glfwGetVideoMode`.
    pub fn get_video_mode(&self) -> Option<VidMode> {
        unsafe {
            // TODO: Can be returned to as_ref + map as in previous commit when (if?) as_ref
            // stabilizes.
            let ptr = ffi::glfwGetVideoMode(self.ptr);
            if ptr.is_null() {
                None
            } else {
                Some(VidMode::from_glfw_vid_mode(&*ptr))
            }
        }
    }

    /// Wrapper for `glfwSetGamma`.
    pub fn set_gamma(&mut self, gamma: f32) {
        unsafe {
            ffi::glfwSetGamma(self.ptr, gamma as c_float);
        }
    }

    /// Wrapper for `glfwGetGammaRamp`.
    pub fn get_gamma_ramp(&self) -> GammaRamp {
        unsafe {
            let llramp = *ffi::glfwGetGammaRamp(self.ptr);
            GammaRamp {
                red: slice::from_raw_parts(llramp.red as *const c_ushort, llramp.size as usize)
                    .iter()
                    .copied()
                    .collect(),
                green: slice::from_raw_parts(llramp.green as *const c_ushort, llramp.size as usize)
                    .iter()
                    .copied()
                    .collect(),
                blue: slice::from_raw_parts(llramp.blue as *const c_ushort, llramp.size as usize)
                    .iter()
                    .copied()
                    .collect(),
            }
        }
    }

    /// Wrapper for `glfwSetGammaRamp`.
    pub fn set_gamma_ramp(&mut self, ramp: &mut GammaRamp) {
        unsafe {
            ffi::glfwSetGammaRamp(
                self.ptr,
                &ffi::GLFWgammaramp {
                    red: ramp.red.as_mut_ptr(),
                    green: ramp.green.as_mut_ptr(),
                    blue: ramp.blue.as_mut_ptr(),
                    size: ramp.red.len() as u32,
                },
            );
        }
    }

    /// Wrapper for `glfwGetMonitorContentScale`.
    pub fn get_content_scale(&self) -> (f32, f32) {
        unsafe {
            let mut xscale = 0.0_f32;
            let mut yscale = 0.0_f32;
            ffi::glfwGetMonitorContentScale(self.ptr, &mut xscale, &mut yscale);
            (xscale, yscale)
        }
    }

    /// Wrapper for `glfwGetMonitorWorkarea`.
    pub fn get_workarea(&self) -> (i32, i32, i32, i32) {
        unsafe {
            let mut xpos = 0;
            let mut ypos = 0;
            let mut width = 0;
            let mut height = 0;
            ffi::glfwGetMonitorWorkarea(self.ptr, &mut xpos, &mut ypos, &mut width, &mut height);
            (xpos, ypos, width, height)
        }
    }
}

/// Monitor events.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MonitorEvent {
    Connected = ffi::GLFW_CONNECTED,
    Disconnected = ffi::GLFW_DISCONNECTED,
}

impl VidMode {
    fn from_glfw_vid_mode(mode: &ffi::GLFWvidmode) -> VidMode {
        VidMode {
            width: mode.width as u32,
            height: mode.height as u32,
            red_bits: mode.redBits as u32,
            green_bits: mode.greenBits as u32,
            blue_bits: mode.blueBits as u32,
            refresh_rate: mode.refreshRate as u32,
        }
    }
}

impl fmt::Debug for VidMode {
    /// Returns a string representation of the video mode.
    ///
    /// # Returns
    ///
    /// A string in the form:
    ///
    /// ~~~ignore
    /// ~"[width] x [height], [total_bits] ([red_bits] [green_bits] [blue_bits]) [refresh_rate] Hz"
    /// ~~~
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} x {}, {} = {} + {} + {}, {} Hz",
            self.width,
            self.height,
            self.red_bits + self.green_bits + self.blue_bits,
            self.red_bits,
            self.green_bits,
            self.blue_bits,
            self.refresh_rate
        )
    }
}

/// Window hints that can be set using the `window_hint` function.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum WindowHint {
    MousePassthrough(bool),
    /// Specifies the desired bit depth of the red component of the default framebuffer.
    RedBits(Option<u32>),
    /// Specifies the desired bit depth of the green component of the default framebuffer.
    GreenBits(Option<u32>),
    /// Specifies the desired bit depth of the blue component of the default framebuffer.
    BlueBits(Option<u32>),
    /// Specifies the desired bit depth of the alpha component of the default framebuffer.
    AlphaBits(Option<u32>),
    /// Specifies the desired bit depth of the depth component of the default framebuffer.
    DepthBits(Option<u32>),
    /// Specifies the desired bit depth of the stencil component of the default framebuffer.
    StencilBits(Option<u32>),
    /// Specifies the desired bit depth of the red component of the accumulation framebuffer.
    AccumRedBits(Option<u32>),
    /// Specifies the desired bit depth of the green component of the accumulation framebuffer.
    AccumGreenBits(Option<u32>),
    /// Specifies the desired bit depth of the blue component of the accumulation framebuffer.
    AccumBlueBits(Option<u32>),
    /// Specifies the desired bit depth of the alpha component of the accumulation framebuffer.
    AccumAlphaBits(Option<u32>),
    /// Specifies the desired number of auxiliary buffers.
    AuxBuffers(Option<u32>),
    /// Specifies whether to use stereoscopic rendering.
    Stereo(bool),
    /// Specifies the desired number of samples to use for multisampling. Zero
    /// disables multisampling.
    Samples(Option<u32>),
    /// Specifies whether the framebuffer should be sRGB capable.
    SRgbCapable(bool),
    /// Specifies the desired refresh rate for full screen windows. If set to `None`,
    /// the highest available refresh rate will be used.
    ///
    /// This hint is ignored for windowed mode windows.
    RefreshRate(Option<u32>),
    /// Specifies which `ClientApi` to create the context for.
    ClientApi(ClientApiHint),
    /// Specifies the major client API version that the created context must be
    /// compatible with.
    ///
    /// Window creation will fail if the resulting OpenGL version is less than
    /// the one requested.
    ContextVersionMajor(u32),
    /// Specifies the minor client API version that the created context must be
    /// compatible with.
    ///
    /// Window creation will fail if the resulting OpenGL version is less than
    /// the one requested.
    ContextVersionMinor(u32),
    /// Specifies the client API version that the created context must be
    /// compatible with. This is the same as successive calls to `window_hint`
    /// function with the `ContextVersionMajor` and `ContextVersionMinor` hints.
    ///
    /// Window creation will fail if the resulting OpenGL version is less than
    /// the one requested.
    ///
    /// If `ContextVersion(1, 0)` is requested, _most_ drivers will provide the
    /// highest available context.
    ContextVersion(u32, u32),
    /// Specifies the `ContextRobustness` strategy to be used.
    ContextRobustness(ContextRobustnessHint),
    /// Specifies whether the OpenGL context should be forward-compatible, i.e.
    /// one where all functionality deprecated in the requested version of
    /// OpenGL is removed. This may only be used if the requested OpenGL version
    /// is 3.0 or above.
    ///
    /// If another client API is requested, this hint is ignored.
    OpenGlForwardCompat(bool),
    /// Specifies whether to create a debug OpenGL context, which may have
    /// additional error and performance issue reporting functionality.
    ///
    /// If another client API is requested, this hint is ignored.
    OpenGlDebugContext(bool),
    /// Specifies which OpenGL profile to create the context for. If requesting
    /// an OpenGL version below 3.2, `OpenGlAnyProfile` must be used.
    ///
    /// If another client API is requested, this hint is ignored.
    OpenGlProfile(OpenGlProfileHint),
    /// Specifies whether the window will be resizable by the user. Even if this
    /// is set to `false`, the window can still be resized using the
    /// `Window::set_size` function.
    ///
    /// This hint is ignored for fullscreen windows.
    Resizable(bool),
    /// Specifies whether the window will be visible on creation.
    ///
    /// This hint is ignored for fullscreen windows.
    Visible(bool),
    /// Specifies whether the window will have platform-specific decorations
    /// such as a border, a close widget, etc.
    ///
    /// This hint is ignored for full screen windows.
    Decorated(bool),
    /// Specifies whether the (full screen) window will automatically iconify
    /// and restore the previous video mode on input focus loss.
    ///
    /// This hint is ignored for windowed mode windows.
    AutoIconify(bool),
    /// Specifies whether the window will be floating above other regular
    /// windows, also called topmost or always-on-top.
    ///
    /// This hint is ignored for full screen windows.
    Floating(bool),
    /// Specifies whether the windowed mode window will be given input focus when created.
    ///
    /// This hint is ignored for full screen and initially hidden windows.
    Focused(bool),
    /// Specifies whether the windowed mode window will be maximized when created.
    ///
    /// This hint is ignored for full screen windows.
    Maximized(bool),
    /// Specifies whether the OpenGL or OpenGL ES contexts do not emit errors,
    /// allowing for better performance in some situations.
    ContextNoError(bool),
    /// Specifies which context creation API to use to create the context.
    ContextCreationApi(ContextCreationApi),
    /// Specifies the behavior of the OpenGL pipeline when a context is transferred between threads
    ContextReleaseBehavior(ContextReleaseBehavior),
    /// Specifies whether the framebuffer should be double buffered.
    ///
    /// You nearly always want to use double buffering.
    ///
    /// Note that setting this to false will make `swap_buffers` do nothing useful,
    /// and your scene will have to be displayed some other way.
    DoubleBuffer(bool),
    /// Speficies whether the cursor should be centered over newly created full screen windows.
    ///
    /// This hint is ignored for windowed mode windows.
    CenterCursor(bool),
    /// Specifies whether the window framebuffer will be transparent.
    ///
    /// If enabled and supported by the system, the window framebuffer alpha channel will be used
    /// to combine the framebuffer with the background. This does not affect window decorations.
    TransparentFramebuffer(bool),
    /// Specifies whether the window will be given input focus when `Window::show` is called.
    FocusOnShow(bool),
    /// Specifies whether the window content area should be resized based on the monitor current
    /// scale of any monitor it is placed on.
    ///
    /// This includes the initial placement when the window is created.
    ScaleToMonitor(bool),
    /// Specifies whether to use full resolution framebuffers on Retina displays.
    ///
    /// This is ignored on platforms besides macOS.
    CocoaRetinaFramebuffer(bool),
    /// Specifies the UTF-8 encoded name to use for autosaving the window frame, or if empty
    /// disables frame autosaving for the window.
    ///
    /// This is ignored on platforms besides macOS.
    CocoaFrameName(Option<String>),
    /// Specifies whether to in participate in Automatic Graphics Switching, i.e. to allow the
    /// system to choose the integrated GPU for the OpenGL context and move it between GPUs if
    /// necessary or whether to force it to always run on the discrete GPU.
    ///
    /// Simpler programs and tools may want to enable this to save power, while games and other
    /// applications performing advanced rendering will want to leave it disabled.
    //
    //  A bundled application that wishes to participate in Automatic Graphics Switching should also
    // declare this in its `Info.plist` by setting the `NSSupportsAutomaticGraphicsSwitching` key to
    // `true`.
    ///
    /// This only affects systems with both integrated and discrete GPUs. This is ignored on
    /// platforms besides macOS.
    CocoaGraphicsSwitching(bool),
    /// Specifies the desired ASCII-encoded class part of the ICCCM `WM_CLASS` window property.
    X11ClassName(Option<String>),
    /// Specifies the desired ASCII-encoded instance part of the ICCCM `WM_CLASS` window property.
    X11InstanceName(Option<String>),
}

/// Client API tokens.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ClientApiHint {
    NoApi = ffi::GLFW_NO_API,
    OpenGl = ffi::GLFW_OPENGL_API,
    OpenGlEs = ffi::GLFW_OPENGL_ES_API,
}

/// Context robustness tokens.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ContextRobustnessHint {
    NoRobustness = ffi::GLFW_NO_ROBUSTNESS,
    NoResetNotification = ffi::GLFW_NO_RESET_NOTIFICATION,
    LoseContextOnReset = ffi::GLFW_LOSE_CONTEXT_ON_RESET,
}

/// OpenGL profile tokens.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum OpenGlProfileHint {
    Any = ffi::GLFW_OPENGL_ANY_PROFILE,
    Core = ffi::GLFW_OPENGL_CORE_PROFILE,
    Compat = ffi::GLFW_OPENGL_COMPAT_PROFILE,
}

/// Describes the mode of a window
#[derive(Copy, Clone, Debug)]
pub enum WindowMode<'a> {
    /// Full screen mode. Contains the monitor on which the window is displayed.
    FullScreen(&'a Monitor),

    /// Windowed mode.
    Windowed,
}

/// Private conversion methods for `glfw::WindowMode`
impl<'a> WindowMode<'a> {
    /// Returns a pointer to a monitor if the window is fullscreen, otherwise
    /// it returns a null pointer (if it is in windowed mode).
    fn to_ptr(&self) -> *mut ffi::GLFWmonitor {
        match *self {
            WindowMode::FullScreen(monitor) => monitor.ptr,
            WindowMode::Windowed => ptr::null_mut(),
        }
    }
}

bitflags! {
    #[doc = "Key modifiers (e.g., Shift, Control, Alt, Super)"]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    pub struct Modifiers: ::std::os::raw::c_int {
        const Shift       = crate::ffi::GLFW_MOD_SHIFT;
        const Control     = crate::ffi::GLFW_MOD_CONTROL;
        const Alt         = crate::ffi::GLFW_MOD_ALT;
        const Super       = crate::ffi::GLFW_MOD_SUPER;
        const CapsLock    = crate::ffi::GLFW_MOD_CAPS_LOCK;
        const NumLock     = crate::ffi::GLFW_MOD_NUM_LOCK;
    }
}

/// Keyboard code returned by the OS
pub type Scancode = c_int;

/// Window event messages.
#[derive(Clone, PartialEq, PartialOrd, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum WindowEvent {
    Pos(i32, i32),
    Size(i32, i32),
    Close,
    Refresh,
    Focus(bool),
    Iconify(bool),
    FramebufferSize(i32, i32),
    MouseButton(MouseButton, Action, Modifiers),
    CursorPos(f64, f64),
    CursorEnter(bool),
    Scroll(f64, f64),
    Key(Key, Scancode, Action, Modifiers),
    Char(char),
    CharModifiers(char, Modifiers),
    FileDrop(Vec<PathBuf>),
    Maximize(bool),
    ContentScale(f32, f32),
}

/// Returns an iterator that yields until no more messages are contained in the
/// `Receiver`'s queue. This is useful for event handling where the blocking
/// behaviour of `Receiver::iter` is undesirable.
///
/// # Example
///
/// ~~~ignore
/// for event in glfw::flush_messages(&events) {
///     // handle event
/// }
/// ~~~
pub fn flush_messages<Message: Send>(
    receiver: &GlfwReceiver<Message>,
) -> FlushedMessages<'_, Message> {
    FlushedMessages(receiver)
}

/// An iterator that yields until no more messages are contained in the
/// `Receiver`'s queue.
#[derive(Debug)]
pub struct FlushedMessages<'a, Message: Send>(&'a GlfwReceiver<Message>);

unsafe impl<'a, Message: 'a + Send> Send for FlushedMessages<'a, Message> {}

impl<'a, Message: 'static + Send> Iterator for FlushedMessages<'a, Message> {
    type Item = Message;

    fn next(&mut self) -> Option<Message> {
        let FlushedMessages(receiver) = *self;
        receiver.receive()
    }
}

/// A struct that wraps a `*GLFWwindow` handle.
#[derive(Debug)]
pub struct Window {
    ptr: *mut ffi::GLFWwindow,
    pub is_shared: bool,
    /// A `Sender` that can be cloned out to child `RenderContext`s.
    drop_sender: Option<Sender<()>>,
    /// Once all  child`RenderContext`s have been dropped, calling `try_recv()`
    /// on the `drop_receiver` will result in an `Err(std::comm::Disconnected)`,
    /// indicating that it is safe to drop the `Window`.
    #[allow(unused)]
    drop_receiver: Receiver<()>,
    /// This is here to allow owning the current Cursor object instead
    /// of forcing the user to take care of its lifetime.
    current_cursor: Option<Cursor>,
    pub glfw: Glfw,
}

impl Window {
    /// Returns the address of the specified client API or extension function if
    /// it is supported by the context associated with this Window. If this Window is not the
    /// current context, it will make it the current context.
    ///
    /// Wrapper for `glfwGetProcAddress`.
    pub fn get_proc_address(&mut self, procname: &str) -> GLProc {
        if self.ptr != unsafe { ffi::glfwGetCurrentContext() } {
            self.make_current();
        }

        self.glfw.get_proc_address_raw(procname)
    }

    /// This function returns the address of the specified Vulkan core or extension function
    /// for the specified instance. If instance is set to NULL it can return any function
    /// exported from the Vulkan loader, including at least the following functions:
    ///
    /// * `vkEnumerateInstanceExtensionProperties`
    /// * `vkEnumerateInstanceLayerProperties`
    /// * `vkCreateInstance`
    /// * `vkGetInstanceProcAddr`
    ///
    /// If Vulkan is not available on the machine, this function returns `NULL`
    ///
    /// Wrapper for `glfwGetInstanceProcAddress`
    #[cfg(feature = "vulkan")]
    pub fn get_instance_proc_address(
        &mut self,
        instance: ffi::VkInstance,
        procname: &str,
    ) -> VkProc {
        self.glfw.get_instance_proc_address_raw(instance, procname)
    }

    /// This function returns whether the specified queue family of the specified
    /// physical device supports presentation to the platform GLFW was built for.
    ///
    /// Wrapper for `glfwGetPhysicalDevicePresentationSupport`
    #[cfg(feature = "vulkan")]
    pub fn get_physical_device_presentation_support(
        &self,
        instance: ffi::VkInstance,
        device: ffi::VkPhysicalDevice,
        queue_family: u32,
    ) -> bool {
        self.glfw
            .get_physical_device_presentation_support_raw(instance, device, queue_family)
    }

    /// wrapper for `glfwCreateWindowSurface`
    #[cfg(feature = "vulkan")]
    pub unsafe fn create_window_surface(
        &self,
        instance: ffi::VkInstance,
        allocator: *const ffi::VkAllocationCallbacks,
        surface: *mut ffi::VkSurfaceKHR,
    ) -> ffi::VkResult {
        unsafe { ffi::glfwCreateWindowSurface(instance, self.ptr, allocator, surface) }
    }

    /// Creates a new shared window.
    ///
    /// Wrapper for `glfwCreateWindow`.
    pub fn create_shared(
        &self,
        width: u32,
        height: u32,
        title: &str,
        mode: WindowMode<'_>,
    ) -> Option<(PWindow, GlfwReceiver<(f64, WindowEvent)>)> {
        self.glfw
            .create_window_intern(width, height, title, mode, Some(self))
    }

    /// Calling this method forces the destructor to be called, closing the
    /// window.
    pub fn close(self) {}

    /// Returns a render context that can be shared between tasks, allowing
    /// for concurrent rendering.
    pub fn render_context(&mut self) -> PRenderContext {
        PRenderContext(Box::new(RenderContext {
            ptr: self.ptr,
            glfw: self.glfw.clone(),
            // this will only be None after dropping so this is safe
            drop_sender: self.drop_sender.as_ref().unwrap().clone(),
        }))
    }

    /// Wrapper for `glfwWindowShouldClose`.
    pub fn should_close(&self) -> bool {
        unsafe { ffi::glfwWindowShouldClose(self.ptr) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwSetWindowShouldClose`.
    pub fn set_should_close(&mut self, value: bool) {
        unsafe { ffi::glfwSetWindowShouldClose(self.ptr, value as c_int) }
    }

    /// Sets the title of the window.
    ///
    /// Wrapper for `glfwSetWindowTitle`.
    pub fn set_title(&mut self, title: &str) {
        unsafe {
            with_c_str(title, |title| {
                ffi::glfwSetWindowTitle(self.ptr, title);
            });
        }
    }

    /// Wrapper for `glfwGetWindowPos`.
    pub fn get_pos(&self) -> (i32, i32) {
        unsafe {
            let mut xpos = 0;
            let mut ypos = 0;
            ffi::glfwGetWindowPos(self.ptr, &mut xpos, &mut ypos);
            (xpos as i32, ypos as i32)
        }
    }

    /// Wrapper for `glfwSetWindowPos`.
    pub fn set_pos(&mut self, xpos: i32, ypos: i32) {
        unsafe {
            ffi::glfwSetWindowPos(self.ptr, xpos as c_int, ypos as c_int);
        }
    }

    /// Wrapper for `glfwGetWindowSize`.
    pub fn get_size(&self) -> (i32, i32) {
        unsafe {
            let mut width = 0;
            let mut height = 0;
            ffi::glfwGetWindowSize(self.ptr, &mut width, &mut height);
            (width as i32, height as i32)
        }
    }

    /// Wrapper for `glfwSetWindowSize`.
    pub fn set_size(&mut self, width: i32, height: i32) {
        unsafe {
            ffi::glfwSetWindowSize(self.ptr, width as c_int, height as c_int);
        }
    }

    /// Wrapper for `glfwGetWindowFrameSize`
    ///
    /// Returns `(left, top, right, bottom)` edge window frame sizes, in screen coordinates.
    pub fn get_frame_size(&self) -> (i32, i32, i32, i32) {
        let (mut left, mut top, mut right, mut bottom): (i32, i32, i32, i32) = (0, 0, 0, 0);

        unsafe {
            ffi::glfwGetWindowFrameSize(
                self.ptr,
                &mut left as *mut c_int,
                &mut top as *mut c_int,
                &mut right as *mut c_int,
                &mut bottom as *mut c_int,
            );
        }

        (left, top, right, bottom)
    }

    /// Wrapper for `glfwGetFramebufferSize`.
    pub fn get_framebuffer_size(&self) -> (i32, i32) {
        unsafe {
            let mut width = 0;
            let mut height = 0;
            ffi::glfwGetFramebufferSize(self.ptr, &mut width, &mut height);
            (width as i32, height as i32)
        }
    }

    /// Wrapper for `glfwSetWindowAspectRatio`.
    pub fn set_aspect_ratio(&mut self, numer: u32, denum: u32) {
        unsafe { ffi::glfwSetWindowAspectRatio(self.ptr, numer as c_int, denum as c_int) }
    }

    /// Wrapper for `glfwSetWindowSizeLimits`.
    ///
    /// A value of `None` is equivalent to `GLFW_DONT_CARE`.
    /// If `minwidth` or `minheight` are `None`, no minimum size is enforced.
    /// If `maxwidth` or `maxheight` are `None`, no maximum size is enforced.
    pub fn set_size_limits(
        &mut self,
        minwidth: Option<u32>,
        minheight: Option<u32>,
        maxwidth: Option<u32>,
        maxheight: Option<u32>,
    ) {
        unsafe {
            ffi::glfwSetWindowSizeLimits(
                self.ptr,
                unwrap_dont_care(minwidth),
                unwrap_dont_care(minheight),
                unwrap_dont_care(maxwidth),
                unwrap_dont_care(maxheight),
            )
        }
    }

    /// Wrapper for `glfwIconifyWindow`.
    pub fn iconify(&mut self) {
        unsafe {
            ffi::glfwIconifyWindow(self.ptr);
        }
    }

    /// Wrapper for `glfwRestoreWindow`.
    pub fn restore(&mut self) {
        unsafe {
            ffi::glfwRestoreWindow(self.ptr);
        }
    }

    /// Wrapper for `glfwMaximizeWindow`
    pub fn maximize(&mut self) {
        unsafe { ffi::glfwMaximizeWindow(self.ptr) }
    }

    /// Wrapper for `glfwShowWindow`.
    pub fn show(&mut self) {
        unsafe {
            ffi::glfwShowWindow(self.ptr);
        }
    }

    /// Wrapper for `glfwHideWindow`.
    pub fn hide(&mut self) {
        unsafe {
            ffi::glfwHideWindow(self.ptr);
        }
    }

    /// Returns whether the window is fullscreen or windowed.
    ///
    /// # Example
    ///
    /// ~~~ignore
    /// window.with_window_mode(|mode| {
    ///     match mode {
    ///         glfw::Windowed => println!("Windowed"),
    ///         glfw::FullScreen(m) => println!("FullScreen({})", m.get_name()),
    ///     }
    /// });
    /// ~~~
    pub fn with_window_mode<T, F>(&self, f: F) -> T
    where
        F: FnOnce(WindowMode<'_>) -> T,
    {
        let ptr = unsafe { ffi::glfwGetWindowMonitor(self.ptr) };
        if ptr.is_null() {
            f(WindowMode::Windowed)
        } else {
            f(WindowMode::FullScreen(&Monitor { ptr }))
        }
    }

    /// Wrapper for `glfwSetWindowMonitor`
    pub fn set_monitor(
        &mut self,
        mode: WindowMode<'_>,
        xpos: i32,
        ypos: i32,
        width: u32,
        height: u32,
        refresh_rate: Option<u32>,
    ) {
        let monitor_ptr = if let WindowMode::FullScreen(monitor) = mode {
            monitor.ptr
        } else {
            ptr::null_mut()
        };

        unsafe {
            ffi::glfwSetWindowMonitor(
                self.ptr,
                monitor_ptr,
                xpos as c_int,
                ypos as c_int,
                width as c_int,
                height as c_int,
                unwrap_dont_care(refresh_rate),
            )
        }
    }

    /// Wrapper for `glfwFocusWindow`
    ///
    /// It is NOT recommended to use this function, as it steals focus from other applications
    /// and can be extremely disruptive to the user.
    pub fn focus(&mut self) {
        unsafe { ffi::glfwFocusWindow(self.ptr) }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `FOCUSED`.
    pub fn is_focused(&self) -> bool {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_FOCUSED) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `ICONIFIED`.
    pub fn is_iconified(&self) -> bool {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_ICONIFIED) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwGetWindowattrib` called with `MAXIMIZED`.
    pub fn is_maximized(&self) -> bool {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_MAXIMIZED) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `CLIENT_API`.
    pub fn get_client_api(&self) -> c_int {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_CLIENT_API) }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with
    /// `CONTEXT_VERSION_MAJOR`, `CONTEXT_VERSION_MINOR` and `CONTEXT_REVISION`.
    ///
    /// # Returns
    ///
    /// The client API version of the window's context in a version struct.
    pub fn get_context_version(&self) -> Version {
        unsafe {
            Version {
                major: ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_CONTEXT_VERSION_MAJOR) as u64,
                minor: ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_CONTEXT_VERSION_MINOR) as u64,
                patch: ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_CONTEXT_REVISION) as u64,
            }
        }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `CONTEXT_ROBUSTNESS`.
    pub fn get_context_robustness(&self) -> c_int {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_CONTEXT_ROBUSTNESS) }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `OPENGL_FORWARD_COMPAT`.
    pub fn is_opengl_forward_compat(&self) -> bool {
        unsafe {
            ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_OPENGL_FORWARD_COMPAT) == ffi::GLFW_TRUE
        }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `OPENGL_DEBUG_CONTEXT`.
    pub fn is_opengl_debug_context(&self) -> bool {
        unsafe {
            ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_OPENGL_DEBUG_CONTEXT) == ffi::GLFW_TRUE
        }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `OPENGL_PROFILE`.
    pub fn get_opengl_profile(&self) -> c_int {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_OPENGL_PROFILE) }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `RESIZABLE`.
    pub fn is_resizable(&self) -> bool {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_RESIZABLE) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwSetWindowAttrib` called with `RESIZABLE`.
    pub fn set_resizable(&mut self, resizable: bool) {
        unsafe { ffi::glfwSetWindowAttrib(self.ptr, ffi::GLFW_RESIZABLE, resizable as c_int) }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `VISIBLE`.
    pub fn is_visible(&self) -> bool {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_VISIBLE) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `DECORATED`.
    pub fn is_decorated(&self) -> bool {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_DECORATED) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwSetWindowAttrib` called with `DECORATED`.
    pub fn set_decorated(&mut self, decorated: bool) {
        unsafe { ffi::glfwSetWindowAttrib(self.ptr, ffi::GLFW_DECORATED, decorated as c_int) }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `AUTO_ICONIFY`.
    pub fn is_auto_iconify(&self) -> bool {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_AUTO_ICONIFY) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwSetWindowAttrib` called with `AUTO_ICONIFY`.
    pub fn set_auto_iconify(&mut self, auto_iconify: bool) {
        unsafe { ffi::glfwSetWindowAttrib(self.ptr, ffi::GLFW_AUTO_ICONIFY, auto_iconify as c_int) }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `FLOATING`.
    pub fn is_floating(&self) -> bool {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_FLOATING) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwSetWindowAttrib` called with `FLOATING`.
    pub fn set_floating(&mut self, floating: bool) {
        unsafe { ffi::glfwSetWindowAttrib(self.ptr, ffi::GLFW_FLOATING, floating as c_int) }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `TRANSPARENT_FRAMEBUFFER`.
    pub fn is_framebuffer_transparent(&self) -> bool {
        unsafe {
            ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_TRANSPARENT_FRAMEBUFFER) == ffi::GLFW_TRUE
        }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `MOUSE_PASSTHROUGH`.
    pub fn is_mouse_passthrough(&self) -> bool {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_MOUSE_PASSTHROUGH) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwSetWindowAttrib` called with `MOUSE_PASSTHROUGH`.
    pub fn set_mouse_passthrough(&mut self, passthrough: bool) {
        unsafe {
            ffi::glfwSetWindowAttrib(self.ptr, ffi::GLFW_MOUSE_PASSTHROUGH, passthrough as c_int);
        }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `FOCUS_ON_SHOW`.
    pub fn is_focus_on_show(&self) -> bool {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_FOCUS_ON_SHOW) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwSetWindowAttrib` called with `FOCUS_ON_SHOW`.
    pub fn set_focus_on_show(&mut self, focus_on_show: bool) {
        unsafe {
            ffi::glfwSetWindowAttrib(self.ptr, ffi::GLFW_FOCUS_ON_SHOW, focus_on_show as c_int)
        }
    }

    /// Wrapper for `glfwGetWindowAttrib` called with `HOVERED`.
    pub fn is_hovered(&self) -> bool {
        unsafe { ffi::glfwGetWindowAttrib(self.ptr, ffi::GLFW_HOVERED) == ffi::GLFW_TRUE }
    }

    new_callback!(
        doc -> "Wrapper for `glfwSetWindowPosCallback`.",
        set -> set_pos_callback,
        unset -> unset_pos_callback,
        poll -> set_pos_polling,
        callback_field -> pos_callback,
        poll_field -> pos_polling,
        window_event -> Pos(i32, i32),
        glfw -> glfwSetWindowPosCallback(x: c_int, y: c_int),
        convert_args -> (x as i32, y as i32),
        secret -> _pos_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetWindowSizeCallback`.",
        set -> set_size_callback,
        unset -> unset_size_callback,
        poll -> set_size_polling,
        callback_field -> size_callback,
        poll_field -> size_polling,
        window_event -> Size(i32, i32),
        glfw -> glfwSetWindowSizeCallback(width: c_int, height: c_int),
        convert_args -> (width as i32, height as i32),
        secret -> _size_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetWindowCloseCallback`.",
        set -> set_close_callback,
        unset -> unset_close_callback,
        poll -> set_close_polling,
        callback_field -> close_callback,
        poll_field -> close_polling,
        window_event -> Close,
        glfw -> glfwSetWindowCloseCallback(),
        convert_args -> (),
        secret -> _close_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetWindowRefreshCallback`.",
        set -> set_refresh_callback,
        unset -> unset_refresh_callback,
        poll -> set_refresh_polling,
        callback_field -> refresh_callback,
        poll_field -> refresh_polling,
        window_event -> Refresh,
        glfw -> glfwSetWindowRefreshCallback(),
        convert_args -> (),
        secret -> _refresh_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetWindowFocusCallback`.",
        set -> set_focus_callback,
        unset -> unset_focus_callback,
        poll -> set_focus_polling,
        callback_field -> focus_callback,
        poll_field -> focus_polling,
        window_event -> Focus(bool),
        glfw -> glfwSetWindowFocusCallback(focused: c_int),
        convert_args -> (focused == ffi::GLFW_TRUE),
        secret -> _focus_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetWindowIconifyCallback`.",
        set -> set_iconify_callback,
        unset -> unset_iconify_callback,
        poll -> set_iconify_polling,
        callback_field -> iconify_callback,
        poll_field -> iconify_polling,
        window_event -> Iconify(bool),
        glfw -> glfwSetWindowIconifyCallback(iconified: c_int),
        convert_args -> (iconified == ffi::GLFW_TRUE),
        secret -> _iconify_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetFramebufferSizeCallback`.",
        set -> set_framebuffer_size_callback,
        unset -> unset_framebuffer_size_callback,
        poll -> set_framebuffer_size_polling,
        callback_field -> framebuffer_size_callback,
        poll_field -> framebuffer_size_polling,
        window_event -> FramebufferSize(i32, i32),
        glfw -> glfwSetFramebufferSizeCallback(width: c_int, height: c_int),
        convert_args -> (width as i32, height as i32),
        secret -> _framebuffer_size_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetKeyCallback`.",
        set -> set_key_callback,
        unset -> unset_key_callback,
        poll -> set_key_polling,
        callback_field -> key_callback,
        poll_field -> key_polling,
        window_event -> Key(Key, Scancode, Action, Modifiers),
        glfw -> glfwSetKeyCallback(key: c_int, scancode: c_int, action: c_int, mods: c_int),
        convert_args -> (
            mem::transmute(key),
            scancode, mem::transmute(action),
            Modifiers::from_bits(mods).unwrap()
        ),
        secret -> _key_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetCharCallback`.",
        set -> set_char_callback,
        unset -> unset_char_callback,
        poll -> set_char_polling,
        callback_field -> char_callback,
        poll_field -> char_polling,
        window_event -> Char(char),
        glfw -> glfwSetCharCallback(character: c_uint),
        convert_args -> (::std::char::from_u32(character).unwrap()),
        secret -> _char_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetCharModsCallback`.",
        set -> set_char_mods_callback,
        unset -> unset_char_mods_callback,
        poll -> set_char_mods_polling,
        callback_field -> char_mods_callback,
        poll_field -> char_mods_polling,
        window_event -> CharModifiers(char, Modifiers),
        glfw -> glfwSetCharModsCallback(character: c_uint, mods: c_int),
        convert_args -> (
            ::std::char::from_u32(character).unwrap(),
            Modifiers::from_bits(mods).unwrap()
        ),
        secret -> _char_mods_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetMouseButtonCallback`.",
        set -> set_mouse_button_callback,
        unset -> unset_mouse_button_callback,
        poll -> set_mouse_button_polling,
        callback_field -> mouse_button_callback,
        poll_field -> mouse_button_polling,
        window_event -> MouseButton(MouseButton, Action, Modifiers),
        glfw -> glfwSetMouseButtonCallback(button: c_int, action: c_int, mods: c_int),
        convert_args -> (
            mem::transmute(button),
            mem::transmute(action),
            Modifiers::from_bits(mods).unwrap()
        ),
        secret -> _mouse_button_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetCursorPosCallback`.",
        set -> set_cursor_pos_callback,
        unset -> unset_cursor_pos_callback,
        poll -> set_cursor_pos_polling,
        callback_field -> cursor_pos_callback,
        poll_field -> cursor_pos_polling,
        window_event -> CursorPos(f64, f64),
        glfw -> glfwSetCursorPosCallback(x: c_double, y: c_double),
        convert_args -> (x as f64, y as f64),
        secret -> _cursor_pos_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetCursorEnterCallback`.",
        set -> set_cursor_enter_callback,
        unset -> unset_cursor_enter_callback,
        poll -> set_cursor_enter_polling,
        callback_field -> cursor_enter_callback,
        poll_field -> cursor_enter_polling,
        window_event -> CursorEnter(bool),
        glfw -> glfwSetCursorEnterCallback(entered: c_int),
        convert_args -> (entered == ffi::GLFW_TRUE),
        secret -> _cursor_enter_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetScrollCallback`.",
        set -> set_scroll_callback,
        unset -> unset_scroll_callback,
        poll -> set_scroll_polling,
        callback_field -> scroll_callback,
        poll_field -> scroll_polling,
        window_event -> Scroll(f64, f64),
        glfw -> glfwSetScrollCallback(x: c_double, y: c_double),
        convert_args -> (x as f64, y as f64),
        secret -> _scroll_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetDropCallback`.",
        set -> set_drag_and_drop_callback,
        unset -> unset_drag_and_drop_callback,
        poll -> set_drag_and_drop_polling,
        callback_field -> drag_and_drop_callback,
        poll_field -> drag_and_drop_polling,
        window_event -> FileDrop(Vec<PathBuf>),
        glfw -> glfwSetDropCallback(num_paths: c_int, paths: *mut *const c_char),
        convert_args -> ({
            slice::from_raw_parts(paths, num_paths as usize)
            .iter()
            .map(|path| PathBuf::from(std::str::from_utf8({
                CStr::from_ptr(*path)
                    .to_bytes()
            })
            .unwrap()
            .to_string()))
            .collect()
        }),
        secret -> _drag_and_drop_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetWindowMaximizeCallback`.",
        set -> set_maximize_callback,
        unset -> unset_maximize_callback,
        poll -> set_maximize_polling,
        callback_field -> maximize_callback,
        poll_field -> maximize_polling,
        window_event -> Maximize(bool),
        glfw -> glfwSetWindowMaximizeCallback(maximized: c_int),
        convert_args -> (maximized == ffi::GLFW_TRUE),
        secret -> _maximize_callback
    );

    new_callback!(
        doc -> "Wrapper for `glfwSetWindowContentScaleCallback`.",
        set -> set_content_scale_callback,
        unset -> unset_content_scale_callback,
        poll -> set_content_scale_polling,
        callback_field -> content_scale_callback,
        poll_field -> content_scale_polling,
        window_event -> ContentScale(f32, f32),
        glfw -> glfwSetWindowContentScaleCallback(xscale: c_float, yscale: c_float),
        convert_args -> (xscale as f32, yscale as f32),
        secret -> _content_scale_callback
    );

    /// Starts or stops polling for all available events
    pub fn set_all_polling(&mut self, should_poll: bool) {
        self.set_pos_polling(should_poll);
        self.set_size_polling(should_poll);
        self.set_close_polling(should_poll);
        self.set_refresh_polling(should_poll);
        self.set_focus_polling(should_poll);
        self.set_iconify_polling(should_poll);
        self.set_framebuffer_size_polling(should_poll);
        self.set_key_polling(should_poll);
        self.set_char_polling(should_poll);
        self.set_char_mods_polling(should_poll);
        self.set_mouse_button_polling(should_poll);
        self.set_cursor_pos_polling(should_poll);
        self.set_cursor_enter_polling(should_poll);
        self.set_scroll_polling(should_poll);
        self.set_drag_and_drop_polling(should_poll);
        self.set_maximize_polling(should_poll);
        self.set_content_scale_polling(should_poll);
    }

    /// Wrapper for `glfwGetInputMode` called with `CURSOR`.
    pub fn get_cursor_mode(&self) -> CursorMode {
        unsafe { mem::transmute(ffi::glfwGetInputMode(self.ptr, ffi::GLFW_CURSOR)) }
    }

    /// Wrapper for `glfwSetInputMode` called with `CURSOR`.
    pub fn set_cursor_mode(&mut self, mode: CursorMode) {
        unsafe {
            ffi::glfwSetInputMode(self.ptr, ffi::GLFW_CURSOR, mode as c_int);
        }
    }

    /// Wrapper for `glfwSetCursor` using `Cursor`
    ///
    /// The window will take ownership of the cursor, and will not Drop it
    /// until it is replaced or the window itself is destroyed.
    ///
    /// Returns the previously set Cursor or None if no cursor was set.
    pub fn set_cursor(&mut self, cursor: Option<Cursor>) -> Option<Cursor> {
        let previous = mem::replace(&mut self.current_cursor, cursor);

        unsafe {
            ffi::glfwSetCursor(
                self.ptr,
                match self.current_cursor {
                    Some(ref cursor) => cursor.ptr,
                    None => ptr::null_mut(),
                },
            )
        }

        previous
    }

    /// Sets the window icon from the given images by called `glfwSetWindowIcon`
    ///
    /// Multiple images can be specified for allowing the OS to choose the best size where
    /// necessary.
    ///
    /// Example:
    ///
    /// ```ignore
    /// if let DynamicImage::ImageRgba8(icon) = image::open("examples/icon.png").unwrap() {
    ///    window.set_icon(vec![
    ///        imageops::resize(&icon, 16, 16, image::imageops::Lanczos3),
    ///        imageops::resize(&icon, 32, 32, image::imageops::Lanczos3),
    ///        imageops::resize(&icon, 48, 48, image::imageops::Lanczos3)
    ///    ]);
    /// }
    /// ```
    #[cfg(feature = "image")]
    pub fn set_icon(&mut self, images: Vec<image::RgbaImage>) {
        // When the images are turned into Vecs, the lifetimes of them go into the Vec lifetime
        // So they need to be kept until the function ends.
        let image_data: Vec<(Vec<_>, u32, u32)> = images
            .into_iter()
            .map(|image| {
                let (width, height) = image.dimensions();

                (image.into_vec(), width, height)
            })
            .collect();

        let glfw_images: Vec<ffi::GLFWimage> = image_data
            .iter()
            .map(|data| ffi::GLFWimage {
                width: data.1 as c_int,
                height: data.2 as c_int,
                pixels: data.0.as_ptr() as _,
            })
            .collect();

        unsafe {
            ffi::glfwSetWindowIcon(
                self.ptr,
                glfw_images.len() as c_int,
                glfw_images.as_ptr() as *const ffi::GLFWimage,
            )
        }
    }

    /// Sets the window icon via `glfwSetWindowIcon` from a set a set of vectors
    /// containing pixels in RGBA format (one pixel per 32-bit integer)
    pub fn set_icon_from_pixels(&mut self, images: Vec<PixelImage>) {
        let glfw_images: Vec<ffi::GLFWimage> = images
            .iter()
            .map(|image: &PixelImage| ffi::GLFWimage {
                width: image.width as c_int,
                height: image.height as c_int,
                pixels: image.pixels.as_ptr() as _,
            })
            .collect();

        unsafe {
            ffi::glfwSetWindowIcon(
                self.ptr,
                glfw_images.len() as c_int,
                glfw_images.as_ptr() as *const ffi::GLFWimage,
            )
        }
    }

    /// Wrapper for `glfwGetInputMode` called with `STICKY_KEYS`.
    pub fn has_sticky_keys(&self) -> bool {
        unsafe { ffi::glfwGetInputMode(self.ptr, ffi::GLFW_STICKY_KEYS) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwSetInputMode` called with `STICKY_KEYS`.
    pub fn set_sticky_keys(&mut self, value: bool) {
        unsafe {
            ffi::glfwSetInputMode(self.ptr, ffi::GLFW_STICKY_KEYS, value as c_int);
        }
    }

    /// Wrapper for `glfwGetInputMode` called with `STICKY_MOUSE_BUTTONS`.
    pub fn has_sticky_mouse_buttons(&self) -> bool {
        unsafe { ffi::glfwGetInputMode(self.ptr, ffi::GLFW_STICKY_MOUSE_BUTTONS) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwSetInputMode` called with `STICKY_MOUSE_BUTTONS`.
    pub fn set_sticky_mouse_buttons(&mut self, value: bool) {
        unsafe {
            ffi::glfwSetInputMode(self.ptr, ffi::GLFW_STICKY_MOUSE_BUTTONS, value as c_int);
        }
    }

    /// Wrapper for `glfwGetInputMode` called with `LOCK_KEY_MODS`
    pub fn does_store_lock_key_mods(&self) -> bool {
        unsafe { ffi::glfwGetInputMode(self.ptr, ffi::GLFW_LOCK_KEY_MODS) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwSetInputMode` called with `LOCK_KEY_MODS`
    pub fn set_store_lock_key_mods(&mut self, value: bool) {
        unsafe { ffi::glfwSetInputMode(self.ptr, ffi::GLFW_LOCK_KEY_MODS, value as c_int) }
    }

    /// Wrapper for `glfwGetInputMode` called with `RAW_MOUSE_MOTION`
    pub fn uses_raw_mouse_motion(&self) -> bool {
        unsafe { ffi::glfwGetInputMode(self.ptr, ffi::GLFW_RAW_MOUSE_MOTION) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwSetInputMode` called with `RAW_MOUSE_MOTION`
    pub fn set_raw_mouse_motion(&mut self, value: bool) {
        unsafe { ffi::glfwSetInputMode(self.ptr, ffi::GLFW_RAW_MOUSE_MOTION, value as c_int) }
    }

    /// Wrapper for `glfwGetKey`.
    pub fn get_key(&self, key: Key) -> Action {
        unsafe { mem::transmute(ffi::glfwGetKey(self.ptr, key as c_int)) }
    }

    /// Wrapper for `glfwGetMouseButton`.
    pub fn get_mouse_button(&self, button: MouseButton) -> Action {
        unsafe { mem::transmute(ffi::glfwGetMouseButton(self.ptr, button as c_int)) }
    }

    /// Wrapper for `glfwGetCursorPos`.
    pub fn get_cursor_pos(&self) -> (f64, f64) {
        unsafe {
            let mut xpos = 0.0;
            let mut ypos = 0.0;
            ffi::glfwGetCursorPos(self.ptr, &mut xpos, &mut ypos);
            (xpos as f64, ypos as f64)
        }
    }

    /// Wrapper for `glfwSetCursorPos`.
    pub fn set_cursor_pos(&mut self, xpos: f64, ypos: f64) {
        unsafe {
            ffi::glfwSetCursorPos(self.ptr, xpos as c_double, ypos as c_double);
        }
    }

    /// Wrapper for `glfwGetClipboardString`.
    pub fn set_clipboard_string(&mut self, string: &str) {
        unsafe {
            with_c_str(string, |string| {
                ffi::glfwSetClipboardString(self.ptr, string);
            });
        }
    }

    /// Wrapper for `glfwGetClipboardString`.
    pub fn get_clipboard_string(&self) -> Option<String> {
        unsafe { string_from_nullable_c_str(ffi::glfwGetClipboardString(self.ptr)) }
    }

    /// Wrapper for `glfwGetWindowOpacity`.
    pub fn get_opacity(&self) -> f32 {
        unsafe { ffi::glfwGetWindowOpacity(self.ptr) }
    }

    /// Wrapper for `glfwSetWindowOpacity`.
    pub fn set_opacity(&mut self, opacity: f32) {
        unsafe { ffi::glfwSetWindowOpacity(self.ptr, opacity) }
    }

    /// Wrapper for `glfwRequestWindowAttention`.
    pub fn request_attention(&mut self) {
        unsafe { ffi::glfwRequestWindowAttention(self.ptr) }
    }

    /// Wrapper for `glfwGetWindowContentScale`.
    pub fn get_content_scale(&self) -> (f32, f32) {
        unsafe {
            let mut xscale = 0.0_f32;
            let mut yscale = 0.0_f32;
            ffi::glfwGetWindowContentScale(self.ptr, &mut xscale, &mut yscale);
            (xscale, yscale)
        }
    }

    /// Wrapper for `glfwGetWin32Window`
    #[cfg(target_os = "windows")]
    pub fn get_win32_window(&self) -> *mut c_void {
        unsafe { ffi::glfwGetWin32Window(self.ptr) }
    }

    /// Wrapper for `glfwGetWGLContext`
    #[cfg(target_os = "windows")]
    pub fn get_wgl_context(&self) -> *mut c_void {
        unsafe { ffi::glfwGetWGLContext(self.ptr) }
    }

    /// Wrapper for `glfwGetCocoaWindow`
    #[cfg(target_os = "macos")]
    pub fn get_cocoa_window(&self) -> *mut c_void {
        unsafe { ffi::glfwGetCocoaWindow(self.ptr) }
    }

    /// Wrapper for `glfwGetNSGLContext`
    #[cfg(target_os = "macos")]
    pub fn get_nsgl_context(&self) -> *mut c_void {
        unsafe { ffi::glfwGetNSGLContext(self.ptr) }
    }

    /// Wrapper for `glfwGetX11Window`
    #[cfg(all(
        not(target_os = "macos"),
        not(target_os = "windows"),
        not(target_os = "emscripten"),
        feature = "x11",
        feature = "native-handles"
    ))]
    pub fn get_x11_window(&self) -> usize {
        unsafe { ffi::glfwGetX11Window(self.ptr) }
    }

    /// Wrapper for `glfwGetWaylandWindow`
    #[cfg(all(
        not(target_os = "macos"),
        not(target_os = "windows"),
        not(target_os = "emscripten"),
        feature = "wayland",
        feature = "native-handles"
    ))]
    pub fn get_wayland_window(&self) -> *mut c_void {
        unsafe { ffi::glfwGetWaylandWindow(self.ptr) }
    }

    /// Wrapper for `glfwGetGLXContext`
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos"), feature = "x11"))]
    pub fn get_glx_context(&self) -> usize {
        unsafe { ffi::glfwGetGLXContext(self.ptr) }
    }
}

impl Drop for Window {
    /// Closes the window and performs the necessary cleanups. This will block
    /// until all associated `RenderContext`s were also dropped, and emit a
    /// `debug!` message to that effect.
    ///
    /// Wrapper for `glfwDestroyWindow`.
    fn drop(&mut self) {
        drop(self.drop_sender.take());

        // Check if all senders from the child `RenderContext`s have hung up.
        #[cfg(feature = "log")]
        if self.drop_receiver.try_recv() != Err(std::sync::mpsc::TryRecvError::Disconnected) {
            debug!("Attempted to drop a Window before the `RenderContext` was dropped.");
            debug!("Blocking until the `RenderContext` was dropped.");
            let _ = self.drop_receiver.recv();
        }

        if !self.ptr.is_null() {
            unsafe {
                let _: Box<WindowCallbacks> =
                    mem::transmute(ffi::glfwGetWindowUserPointer(self.ptr));
            }
        }

        // flutter-rs patch: upstream skips `glfwDestroyWindow` for windows created
        // with `create_shared`, which leaks the native window and its GL context.
        // GLFW does not destroy shared windows automatically, so always destroy.
        unsafe {
            ffi::glfwDestroyWindow(self.ptr);
        }
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct PRenderContext(Box<RenderContext>);

impl Deref for PRenderContext {
    type Target = RenderContext;
    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl DerefMut for PRenderContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}

unsafe impl Send for PRenderContext {}
unsafe impl Sync for PRenderContext {}

#[cfg(feature = "raw-window-handle-v0-6")]
impl HasWindowHandle for PRenderContext {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

#[cfg(feature = "raw-window-handle-v0-6")]
impl HasDisplayHandle for PRenderContext {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

/// A rendering context that can be shared between tasks.
#[derive(Debug)]
pub struct RenderContext {
    ptr: *mut ffi::GLFWwindow,
    glfw: Glfw,
    /// As long as this sender is alive, it is not safe to drop the parent
    /// `Window`.
    #[allow(dead_code)]
    drop_sender: Sender<()>,
}

impl RenderContext {
    /// Wrapper function, please refer to [`Window::get_proc_address`]
    pub fn get_proc_address(&mut self, procname: &str) -> GLProc {
        if self.ptr != unsafe { ffi::glfwGetCurrentContext() } {
            self.make_current();
        }

        self.glfw.get_proc_address_raw(procname)
    }

    /// Wrapper function, please refer to [`Window::get_instance_proc_address`]
    #[cfg(feature = "vulkan")]
    pub fn get_instance_proc_address(
        &mut self,
        instance: ffi::VkInstance,
        procname: &str,
    ) -> VkProc {
        self.glfw.get_instance_proc_address_raw(instance, procname)
    }

    /// Wrapper function, please refer to [`Window::get_physical_device_presentation_support`]
    #[cfg(feature = "vulkan")]
    pub fn get_physical_device_presentation_support(
        &self,
        instance: ffi::VkInstance,
        device: ffi::VkPhysicalDevice,
        queue_family: u32,
    ) -> bool {
        self.glfw
            .get_physical_device_presentation_support_raw(instance, device, queue_family)
    }

    /// Wrapper function, please refer to [`Window::create_window_surface`]
    #[cfg(feature = "vulkan")]
    pub unsafe fn create_window_surface(
        &self,
        instance: ffi::VkInstance,
        allocator: *const ffi::VkAllocationCallbacks,
        surface: *mut ffi::VkSurfaceKHR,
    ) -> ffi::VkResult {
        unsafe { ffi::glfwCreateWindowSurface(instance, self.ptr, allocator, surface) }
    }
}

unsafe impl Send for RenderContext {}

/// Methods common to renderable contexts
pub trait Context {
    /// Returns the pointer to the underlying `GLFWwindow`.
    fn window_ptr(&self) -> *mut ffi::GLFWwindow;

    /// Returns the unique identifier for this window.
    fn window_id(&self) -> WindowId {
        self.window_ptr() as WindowId
    }

    /// Swaps the front and back buffers of the window. If the swap interval is
    /// greater than zero, the GPU driver waits the specified number of screen
    /// updates before swapping the buffers.
    ///
    /// Wrapper for `glfwSwapBuffers`.
    fn swap_buffers(&mut self) {
        let ptr = self.window_ptr();
        unsafe {
            ffi::glfwSwapBuffers(ptr);
        }
    }

    /// Returns `true` if the window is the current context.
    fn is_current(&self) -> bool {
        self.window_ptr() == unsafe { ffi::glfwGetCurrentContext() }
    }

    /// Wrapper for `glfwMakeContextCurrent`
    fn make_current(&mut self) {
        let ptr = self.window_ptr();
        unsafe {
            ffi::glfwMakeContextCurrent(ptr);
        }
    }

    /// Wrapper for `glfwWindowShouldClose`.
    fn should_close(&self) -> bool {
        let ptr = self.window_ptr();
        unsafe { ffi::glfwWindowShouldClose(ptr) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwSetWindowShouldClose`.
    fn set_should_close(&mut self, value: bool) {
        let ptr = self.window_ptr();
        unsafe {
            ffi::glfwSetWindowShouldClose(ptr, value as c_int);
        }
    }

    /// Wrapper for `glfwPostEmptyEvent`.
    fn post_empty_event(&self) {
        unsafe { ffi::glfwPostEmptyEvent() }
    }
}

impl Context for Window {
    fn window_ptr(&self) -> *mut ffi::GLFWwindow {
        self.ptr
    }
}

impl Context for RenderContext {
    fn window_ptr(&self) -> *mut ffi::GLFWwindow {
        self.ptr
    }
}

#[cfg(feature = "raw-window-handle-v0-6")]
impl HasWindowHandle for Window {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Ok(unsafe { WindowHandle::borrow_raw(raw_window_handle(self)) })
    }
}

#[cfg(feature = "raw-window-handle-v0-6")]
impl HasWindowHandle for RenderContext {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Ok(unsafe { WindowHandle::borrow_raw(raw_window_handle(self)) })
    }
}

#[cfg(feature = "raw-window-handle-v0-6")]
impl HasDisplayHandle for Window {
    fn display_handle(&'_ self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe { DisplayHandle::borrow_raw(raw_display_handle()) })
    }
}

#[cfg(feature = "raw-window-handle-v0-6")]
impl HasDisplayHandle for RenderContext {
    fn display_handle(&'_ self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe { DisplayHandle::borrow_raw(raw_display_handle()) })
    }
}

#[cfg(feature = "raw-window-handle-v0-5")]
unsafe impl HasRawWindowHandle for Window {
    fn raw_window_handle(&self) -> RawWindowHandle {
        raw_window_handle(self)
    }
}

#[cfg(feature = "raw-window-handle-v0-5")]
unsafe impl HasRawWindowHandle for RenderContext {
    fn raw_window_handle(&self) -> RawWindowHandle {
        raw_window_handle(self)
    }
}

#[cfg(feature = "raw-window-handle-v0-5")]
unsafe impl HasRawDisplayHandle for Window {
    fn raw_display_handle(&self) -> RawDisplayHandle {
        raw_display_handle()
    }
}

#[cfg(feature = "raw-window-handle-v0-5")]
unsafe impl HasRawDisplayHandle for RenderContext {
    fn raw_display_handle(&self) -> RawDisplayHandle {
        raw_display_handle()
    }
}

#[cfg(feature = "raw-window-handle-v0-6")]
fn raw_window_handle<C: Context>(context: &C) -> RawWindowHandle {
    cfg_select! {
        target_family = "windows" => {
            use std::num::NonZeroIsize;

            use raw_window_handle::Win32WindowHandle;
            let (hwnd, hinstance): (*mut std::ffi::c_void, *mut std::ffi::c_void) = unsafe {
                let hwnd = ffi::glfwGetWin32Window(context.window_ptr());
                let hinstance: *mut c_void =
                    winapi::um::libloaderapi::GetModuleHandleW(std::ptr::null()) as _;
                (hwnd, hinstance as _)
            };
            let mut handle = Win32WindowHandle::new(NonZeroIsize::new(hwnd as isize).unwrap());
            handle.hinstance = NonZeroIsize::new(hinstance as isize);
            RawWindowHandle::Win32(handle)
        },
        any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly") => {
            cfg_select! {
                any(feature = "x11", feature = "wayland") => {
                    let platform = unsafe { mem::transmute::<i32, Platform>(ffi::glfwGetPlatform()) };
                    match platform {
                        #[cfg(feature = "x11")]
                        Platform::X11 => {
                            use raw_window_handle::XlibWindowHandle;
                            let window = unsafe {
                                ffi::glfwGetX11Window(context.window_ptr()) as std::os::raw::c_ulong
                            };
                            RawWindowHandle::Xlib(XlibWindowHandle::new(window))
                        }
                        #[cfg(feature = "wayland")]
                        Platform::Wayland => {
                            use std::ptr::NonNull;
                            use raw_window_handle::WaylandWindowHandle;
                            let surface = unsafe { ffi::glfwGetWaylandWindow(context.window_ptr()) };
                            let handle = WaylandWindowHandle::new(
                                NonNull::new(surface).expect("wayland window surface is null"),
                            );
                            RawWindowHandle::Wayland(handle)
                        }
                        _ => panic!("Unsupported platform: {:?}", platform),
                    }
                },
                _ => compile_error!("either the `x11` or `wayland` feature must be enabled on linux")
            }
        },
        target_os = "macos" => {
            use std::ptr::NonNull;

            use objc2::msg_send_id;
            use objc2::rc::Id;
            use objc2::runtime::NSObject;
            use raw_window_handle::AppKitWindowHandle;
            let ns_window: *mut NSObject =
                unsafe { ffi::glfwGetCocoaWindow(context.window_ptr()) as *mut _ };
            let ns_view: Option<Id<NSObject>> = unsafe { msg_send_id![ns_window, contentView] };
            let ns_view = ns_view.expect("failed to access contentView on GLFW NSWindow");
            let ns_view: NonNull<NSObject> = NonNull::from(&*ns_view);
            let handle = AppKitWindowHandle::new(ns_view.cast());
            RawWindowHandle::AppKit(handle)
        },
        target_os = "emscripten" => {
            let _ = context; // to avoid unused lint
            let mut wh = raw_window_handle::WebWindowHandle::new(1);
            // glfw on emscripten only supports a single window. so, just hardcode it
            // sdl2 crate does the same. users can just add `data-raw-handle="1"` attribute to their
            // canvas element
            RawWindowHandle::Web(wh)
        },
        _ => compile_error!("unsupported operating system with `raw-window-handle-v0-6` enabled")
    }
}

#[cfg(feature = "raw-window-handle-v0-6")]
fn raw_display_handle() -> RawDisplayHandle {
    cfg_select! {
        target_family = "windows" => {
                use raw_window_handle::WindowsDisplayHandle;
            RawDisplayHandle::Windows(WindowsDisplayHandle::new())
        },
        any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly") => {
            cfg_select! {
                any(feature = "x11", feature = "wayland") => {
                    let platform = unsafe { mem::transmute::<i32, Platform>(ffi::glfwGetPlatform()) };
                    match platform {
                        #[cfg(feature = "x11")]
                        Platform::X11 => {
                            use std::ptr::NonNull;
                            use raw_window_handle::XlibDisplayHandle;
                            let display = NonNull::new(unsafe { ffi::glfwGetX11Display() });
                            let handle = XlibDisplayHandle::new(display, 0);
                            RawDisplayHandle::Xlib(handle)
                        }
                        #[cfg(feature = "wayland")]
                        Platform::Wayland => {
                            use std::ptr::NonNull;
                            use raw_window_handle::WaylandDisplayHandle;
                            let display = NonNull::new(unsafe { ffi::glfwGetWaylandDisplay().cast_mut() })
                                .expect("wayland display is null");
                            let handle = WaylandDisplayHandle::new(display);
                            RawDisplayHandle::Wayland(handle)
                        }
                        _ => panic!("Unsupported platform: {:?}", platform),
                    }
                },
                _ => compile_error!("either the `x11` or `wayland` feature must be enabled on linux")
            }
        },
        target_os = "macos" => {
            use raw_window_handle::AppKitDisplayHandle;
            RawDisplayHandle::AppKit(AppKitDisplayHandle::new())
        },
        target_os = "emscripten" => {
            RawDisplayHandle::Web(raw_window_handle::WebDisplayHandle::new())
        },
        _ => compile_error!("unsupported operating system with `raw-window-handle-v0-6` enabled")
    }
}

#[cfg(feature = "raw-window-handle-v0-5")]
fn raw_window_handle<C: Context>(context: &C) -> RawWindowHandle {
    cfg_select! {
        target_family = "windows" => {
            use raw_window_handle::Win32WindowHandle;
            let (hwnd, hinstance) = unsafe {
                let hwnd = ffi::glfwGetWin32Window(context.window_ptr());
                let hinstance = winapi::um::libloaderapi::GetModuleHandleW(std::ptr::null());
                (hwnd, hinstance as _)
            };
            let mut handle = Win32WindowHandle::empty();
            handle.hwnd = hwnd;
            handle.hinstance = hinstance;
            RawWindowHandle::Win32(handle)
        },
        any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly") => {
            cfg_select! {
                any(feature = "x11", feature = "wayland") => {
                    let platform = unsafe { mem::transmute::<i32, Platform>(ffi::glfwGetPlatform()) };
                    match platform {
                        #[cfg(feature = "x11")]
                        Platform::X11 => {
                            use raw_window_handle::XlibWindowHandle;
                            let mut handle = XlibWindowHandle::empty();
                            handle.window = unsafe {
                                ffi::glfwGetX11Window(context.window_ptr()) as std::os::raw::c_ulong
                            };
                            RawWindowHandle::Xlib(handle)
                        }
                        #[cfg(feature = "wayland")]
                        Platform::Wayland => {
                            use raw_window_handle::WaylandWindowHandle;
                            let mut handle = WaylandWindowHandle::empty();
                            handle.surface = unsafe { ffi::glfwGetWaylandWindow(context.window_ptr()) };
                            RawWindowHandle::Wayland(handle)
                        }
                        _ => panic!("Unsupported platform: {:?}", platform),
                    }
                },
                _ => compile_error!("either the `x11` or `wayland` feature must be enabled on linux")
            }
        },
        target_os = "macos" => {
            use raw_window_handle::AppKitWindowHandle;
            let (ns_window, ns_view) = unsafe {
                let ns_window: *mut objc::runtime::Object =
                    ffi::glfwGetCocoaWindow(context.window_ptr()) as *mut _;
                let ns_view: *mut objc::runtime::Object = objc::msg_send![ns_window, contentView];
                assert_ne!(ns_view, std::ptr::null_mut());
                (
                    ns_window as *mut std::ffi::c_void,
                    ns_view as *mut std::ffi::c_void,
                )
            };
            let mut handle = AppKitWindowHandle::empty();
            handle.ns_window = ns_window;
            handle.ns_view = ns_view;
            RawWindowHandle::AppKit(handle)
            },
            target_os = "emscripten" => {
                let _ = context; // to avoid unused lint
            let mut wh = raw_window_handle::WebWindowHandle::empty();
            // glfw on emscripten only supports a single window. so, just hardcode it
            // sdl2 crate does the same. users can just add `data-raw-handle="1"` attribute to their
            // canvas element
            wh.id = 1;
            RawWindowHandle::Web(wh)
        }
        _ => compile_error!("selected os is not supported with feature `raw-window-handle-v0-5` enabled")
    }
}

#[cfg(feature = "raw-window-handle-v0-5")]
fn raw_display_handle() -> RawDisplayHandle {
    cfg_select! {
        target_family = "windows" => {
            use raw_window_handle::WindowsDisplayHandle;
            RawDisplayHandle::Windows(WindowsDisplayHandle::empty())
        },
        any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly") => {
            cfg_select! {
                any(feature = "x11", feature = "wayland") => {
                    let platform = unsafe { mem::transmute::<i32, Platform>(ffi::glfwGetPlatform()) };
                    match platform {
                        #[cfg(feature = "x11")]
                        Platform::X11 => {
                            use raw_window_handle::XlibDisplayHandle;
                            let mut handle = XlibDisplayHandle::empty();
                            handle.display = unsafe { ffi::glfwGetX11Display() };
                            RawDisplayHandle::Xlib(handle)
                        }
                        #[cfg(feature = "wayland")]
                        Platform::Wayland => {
                            use raw_window_handle::WaylandDisplayHandle;
                            let mut handle = WaylandDisplayHandle::empty();
                            // the display expects a mutable pointer, but we have a const one
                            handle.display = unsafe { ffi::glfwGetWaylandDisplay().cast_mut() };
                            RawDisplayHandle::Wayland(handle)
                        }
                        _ => panic!("Unsupported platform: {:?}", platform),
                    }
                },
                _ => compile_error!("either the `x11` or `wayland` feature must be enabled on linux")
            }
        },
        target_os = "macos" => {
            use raw_window_handle::AppKitDisplayHandle;
            RawDisplayHandle::AppKit(AppKitDisplayHandle::empty())
        },
        target_os = "emscripten" => {
            RawDisplayHandle::Web(raw_window_handle::WebDisplayHandle::empty())
        },
        _ => compile_error!("unsupported operating system with `raw-window-handle-v0-5` enabled")
    }
}

/// Wrapper for `glfwMakeContextCurrent`.
pub fn make_context_current(context: Option<&dyn Context>) {
    match context {
        Some(ctx) => unsafe { ffi::glfwMakeContextCurrent(ctx.window_ptr()) },
        None => unsafe { ffi::glfwMakeContextCurrent(ptr::null_mut()) },
    }
}

/// Joystick identifier tokens.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum JoystickId {
    Joystick1 = ffi::GLFW_JOYSTICK_1,
    Joystick2 = ffi::GLFW_JOYSTICK_2,
    Joystick3 = ffi::GLFW_JOYSTICK_3,
    Joystick4 = ffi::GLFW_JOYSTICK_4,
    Joystick5 = ffi::GLFW_JOYSTICK_5,
    Joystick6 = ffi::GLFW_JOYSTICK_6,
    Joystick7 = ffi::GLFW_JOYSTICK_7,
    Joystick8 = ffi::GLFW_JOYSTICK_8,
    Joystick9 = ffi::GLFW_JOYSTICK_9,
    Joystick10 = ffi::GLFW_JOYSTICK_10,
    Joystick11 = ffi::GLFW_JOYSTICK_11,
    Joystick12 = ffi::GLFW_JOYSTICK_12,
    Joystick13 = ffi::GLFW_JOYSTICK_13,
    Joystick14 = ffi::GLFW_JOYSTICK_14,
    Joystick15 = ffi::GLFW_JOYSTICK_15,
    Joystick16 = ffi::GLFW_JOYSTICK_16,
}

impl JoystickId {
    /// Converts from `i32`.
    pub fn from_i32(n: i32) -> Option<JoystickId> {
        if (0..=ffi::GLFW_JOYSTICK_LAST).contains(&n) {
            Some(unsafe { mem::transmute(n) })
        } else {
            None
        }
    }
}

/// Button identifier tokens.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum GamepadButton {
    ButtonA = ffi::GLFW_GAMEPAD_BUTTON_A,
    ButtonB = ffi::GLFW_GAMEPAD_BUTTON_B,
    ButtonX = ffi::GLFW_GAMEPAD_BUTTON_X,
    ButtonY = ffi::GLFW_GAMEPAD_BUTTON_Y,
    ButtonLeftBumper = ffi::GLFW_GAMEPAD_BUTTON_LEFT_BUMPER,
    ButtonRightBumper = ffi::GLFW_GAMEPAD_BUTTON_RIGHT_BUMPER,
    ButtonBack = ffi::GLFW_GAMEPAD_BUTTON_BACK,
    ButtonStart = ffi::GLFW_GAMEPAD_BUTTON_START,
    ButtonGuide = ffi::GLFW_GAMEPAD_BUTTON_GUIDE,
    ButtonLeftThumb = ffi::GLFW_GAMEPAD_BUTTON_LEFT_THUMB,
    ButtonRightThumb = ffi::GLFW_GAMEPAD_BUTTON_RIGHT_THUMB,
    ButtonDpadUp = ffi::GLFW_GAMEPAD_BUTTON_DPAD_UP,
    ButtonDpadRight = ffi::GLFW_GAMEPAD_BUTTON_DPAD_RIGHT,
    ButtonDpadDown = ffi::GLFW_GAMEPAD_BUTTON_DPAD_DOWN,
    ButtonDpadLeft = ffi::GLFW_GAMEPAD_BUTTON_DPAD_LEFT,
}

impl GamepadButton {
    /// Converts from `i32`.
    pub fn from_i32(n: i32) -> Option<GamepadButton> {
        if (0..=ffi::GLFW_GAMEPAD_BUTTON_LAST).contains(&n) {
            Some(unsafe { mem::transmute(n) })
        } else {
            None
        }
    }
}

/// Axis identifier tokens.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum GamepadAxis {
    AxisLeftX = ffi::GLFW_GAMEPAD_AXIS_LEFT_X,
    AxisLeftY = ffi::GLFW_GAMEPAD_AXIS_LEFT_Y,
    AxisRightX = ffi::GLFW_GAMEPAD_AXIS_RIGHT_X,
    AxisRightY = ffi::GLFW_GAMEPAD_AXIS_RIGHT_Y,
    AxisLeftTrigger = ffi::GLFW_GAMEPAD_AXIS_LEFT_TRIGGER,
    AxisRightTrigger = ffi::GLFW_GAMEPAD_AXIS_RIGHT_TRIGGER,
}

impl GamepadAxis {
    /// Converts from `i32`.
    pub fn from_i32(n: i32) -> Option<GamepadAxis> {
        if (0..=ffi::GLFW_GAMEPAD_AXIS_LAST).contains(&n) {
            Some(unsafe { mem::transmute(n) })
        } else {
            None
        }
    }
}

bitflags! {
    #[doc = "Joystick hats."]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    pub struct JoystickHats: ::std::os::raw::c_int {
        const Centered = crate::ffi::GLFW_HAT_CENTERED;
        const Up       = crate::ffi::GLFW_HAT_UP;
        const Right    = crate::ffi::GLFW_HAT_RIGHT;
        const Down     = crate::ffi::GLFW_HAT_DOWN;
        const Left     = crate::ffi::GLFW_HAT_LEFT;
    }
}

/// A joystick handle.
#[derive(Clone, Debug)]
pub struct Joystick {
    pub id: JoystickId,
    pub glfw: Glfw,
}

/// State of a gamepad.
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GamepadState {
    buttons: [Action; (ffi::GLFW_GAMEPAD_BUTTON_LAST + 1) as usize],
    axes: [f32; (ffi::GLFW_GAMEPAD_AXIS_LAST + 1) as usize],
}

/// Joystick events.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum JoystickEvent {
    Connected = ffi::GLFW_CONNECTED,
    Disconnected = ffi::GLFW_DISCONNECTED,
}

impl Joystick {
    /// Wrapper for `glfwJoystickPresent`.
    pub fn is_present(&self) -> bool {
        unsafe { ffi::glfwJoystickPresent(self.id as c_int) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwGetJoystickAxes`.
    pub fn get_axes(&self) -> Vec<f32> {
        unsafe {
            let mut count = 0;
            let ptr = ffi::glfwGetJoystickAxes(self.id as c_int, &mut count);
            if ptr.is_null() {
                return Vec::new();
            }
            slice::from_raw_parts(ptr, count as usize)
                .iter()
                .map(|&a| a as f32)
                .collect()
        }
    }

    /// Wrapper for `glfwGetJoystickButtons`.
    pub fn get_buttons(&self) -> Vec<c_int> {
        unsafe {
            let mut count = 0;
            let ptr = ffi::glfwGetJoystickButtons(self.id as c_int, &mut count);
            if ptr.is_null() {
                return Vec::new();
            }
            slice::from_raw_parts(ptr, count as usize)
                .iter()
                .map(|&b| b as c_int)
                .collect()
        }
    }

    /// Wrapper for `glfwGetJoystickHats`.
    pub fn get_hats(&self) -> Vec<JoystickHats> {
        unsafe {
            let mut count = 0;
            let ptr = ffi::glfwGetJoystickHats(self.id as c_int, &mut count);
            if ptr.is_null() {
                return Vec::new();
            }
            slice::from_raw_parts(ptr, count as usize)
                .iter()
                .map(|&b| mem::transmute(b as c_int))
                .collect()
        }
    }

    /// Wrapper for `glfwGetJoystickName`.
    pub fn get_name(&self) -> Option<String> {
        unsafe { string_from_nullable_c_str(ffi::glfwGetJoystickName(self.id as c_int)) }
    }

    /// Wrapper for `glfwGetJoystickGUID`.
    pub fn get_guid(&self) -> Option<String> {
        unsafe { string_from_nullable_c_str(ffi::glfwGetJoystickGUID(self.id as c_int)) }
    }

    /// Wrapper for `glfwJoystickIsGamepad`.
    pub fn is_gamepad(&self) -> bool {
        unsafe { ffi::glfwJoystickIsGamepad(self.id as c_int) == ffi::GLFW_TRUE }
    }

    /// Wrapper for `glfwGetGamepadName`.
    pub fn get_gamepad_name(&self) -> Option<String> {
        unsafe { string_from_nullable_c_str(ffi::glfwGetGamepadName(self.id as c_int)) }
    }

    /// Wrapper for `glfwGetGamepadState`.
    pub fn get_gamepad_state(&self) -> Option<GamepadState> {
        unsafe {
            let mut state = ffi::GLFWgamepadstate {
                buttons: [0; (ffi::GLFW_GAMEPAD_BUTTON_LAST + 1) as usize],
                axes: [0_f32; (ffi::GLFW_GAMEPAD_AXIS_LAST + 1) as usize],
            };
            if ffi::glfwGetGamepadState(self.id as c_int, &mut state) == ffi::GLFW_TRUE {
                Some(state.into())
            } else {
                None
            }
        }
    }
}

impl From<ffi::GLFWgamepadstate> for GamepadState {
    fn from(state: ffi::GLFWgamepadstate) -> Self {
        let mut buttons = [Action::Release; (ffi::GLFW_GAMEPAD_BUTTON_LAST + 1) as usize];
        let mut axes = [0_f32; (ffi::GLFW_GAMEPAD_AXIS_LAST + 1) as usize];
        unsafe {
            state
                .buttons
                .iter()
                .map(|&b| mem::transmute(b as c_int))
                .zip(buttons.iter_mut())
                .for_each(|(a, b)| *b = a);
        }
        state
            .axes
            .iter()
            .map(|&f| f as f32)
            .zip(axes.iter_mut())
            .for_each(|(a, b)| *b = a);
        Self { buttons, axes }
    }
}

impl GamepadState {
    pub fn get_button_state(&self, button: GamepadButton) -> Action {
        self.buttons[button as usize]
    }

    pub fn get_axis(&self, axis: GamepadAxis) -> f32 {
        self.axes[axis as usize]
    }
}

#[inline(always)]
fn unwrap_dont_care(value: Option<u32>) -> c_int {
    match value {
        Some(v) => v as c_int,
        None => ffi::GLFW_DONT_CARE,
    }
}
