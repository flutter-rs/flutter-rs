use flutter_engine_sys::{FlutterPlatformMessage, FlutterPlatformMessageResponseHandle};
use log::error;
use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::{mem, ptr};

#[derive(Debug)]
pub struct PlatformMessageResponseHandle {
    handle: *const FlutterPlatformMessageResponseHandle,
}

unsafe impl Send for PlatformMessageResponseHandle {}

unsafe impl Sync for PlatformMessageResponseHandle {}

impl Into<PlatformMessageResponseHandle> for *const FlutterPlatformMessageResponseHandle {
    fn into(self) -> PlatformMessageResponseHandle {
        PlatformMessageResponseHandle { handle: self }
    }
}

impl From<PlatformMessageResponseHandle> for *const FlutterPlatformMessageResponseHandle {
    fn from(mut handle: PlatformMessageResponseHandle) -> Self {
        let ptr = handle.handle;
        handle.handle = ptr::null();
        ptr
    }
}

impl Drop for PlatformMessageResponseHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            error!("A message response handle has been dropped without sending a response! This WILL lead to leaking memory.");
        }
    }
}

#[derive(Debug)]
pub struct PlatformMessage<'a, 'b> {
    pub channel: Cow<'a, str>,
    pub message: &'b [u8],
    pub response_handle: Option<PlatformMessageResponseHandle>,
}

impl<'a, 'b> Into<FlutterPlatformMessage> for PlatformMessage<'a, 'b> {
    fn into(mut self) -> FlutterPlatformMessage {
        FlutterPlatformMessage {
            struct_size: mem::size_of::<FlutterPlatformMessage>(),
            channel: CString::new(&*self.channel).unwrap().into_raw(),
            message: self.message.as_ptr(),
            message_size: self.message.len(),
            response_handle: self.response_handle.take().map_or(ptr::null(), Into::into),
        }
    }
}

impl<'a, 'b> From<FlutterPlatformMessage> for PlatformMessage<'a, 'b> {
    fn from(platform_message: FlutterPlatformMessage) -> Self {
        debug_assert_eq!(
            platform_message.struct_size,
            mem::size_of::<FlutterPlatformMessage>()
        );
        unsafe {
            let channel = CStr::from_ptr(platform_message.channel).to_string_lossy();
            let message =
                std::slice::from_raw_parts(platform_message.message, platform_message.message_size);
            let response_handle = if platform_message.response_handle.is_null() {
                None
            } else {
                Some(platform_message.response_handle.into())
            };
            Self {
                channel,
                message,
                response_handle,
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FlutterPointerPhase {
    Cancel,
    Up,
    Down,
    Move,
    Add,
    Remove,
    Hover,
}

impl From<FlutterPointerPhase> for flutter_engine_sys::FlutterPointerPhase {
    fn from(pointer_phase: FlutterPointerPhase) -> Self {
        match pointer_phase {
            FlutterPointerPhase::Cancel => flutter_engine_sys::FlutterPointerPhase::kCancel,
            FlutterPointerPhase::Up => flutter_engine_sys::FlutterPointerPhase::kUp,
            FlutterPointerPhase::Down => flutter_engine_sys::FlutterPointerPhase::kDown,
            FlutterPointerPhase::Move => flutter_engine_sys::FlutterPointerPhase::kMove,
            FlutterPointerPhase::Add => flutter_engine_sys::FlutterPointerPhase::kAdd,
            FlutterPointerPhase::Remove => flutter_engine_sys::FlutterPointerPhase::kRemove,
            FlutterPointerPhase::Hover => flutter_engine_sys::FlutterPointerPhase::kHover,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FlutterPointerDeviceKind {
    Mouse,
    Touch,
}

impl From<FlutterPointerDeviceKind> for flutter_engine_sys::FlutterPointerDeviceKind {
    fn from(device_kind: FlutterPointerDeviceKind) -> Self {
        match device_kind {
            FlutterPointerDeviceKind::Mouse => {
                flutter_engine_sys::FlutterPointerDeviceKind::kFlutterPointerDeviceKindMouse
            }
            FlutterPointerDeviceKind::Touch => {
                flutter_engine_sys::FlutterPointerDeviceKind::kFlutterPointerDeviceKindTouch
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FlutterPointerSignalKind {
    None,
    Scroll,
}

impl From<FlutterPointerSignalKind> for flutter_engine_sys::FlutterPointerSignalKind {
    fn from(pointer_signal_kind: FlutterPointerSignalKind) -> Self {
        match pointer_signal_kind {
            FlutterPointerSignalKind::None => {
                flutter_engine_sys::FlutterPointerSignalKind::kFlutterPointerSignalKindNone
            }
            FlutterPointerSignalKind::Scroll => {
                flutter_engine_sys::FlutterPointerSignalKind::kFlutterPointerSignalKindScroll
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FlutterPointerMouseButtons {
    Primary,
    Secondary,
    Middle,
    Back,
    Forward,
}

impl From<FlutterPointerMouseButtons> for flutter_engine_sys::FlutterPointerMouseButtons {
    fn from(btn: FlutterPointerMouseButtons) -> Self {
        match btn {
            FlutterPointerMouseButtons::Primary => {
                flutter_engine_sys::FlutterPointerMouseButtons::kFlutterPointerButtonMousePrimary
            }
            FlutterPointerMouseButtons::Secondary => {
                flutter_engine_sys::FlutterPointerMouseButtons::kFlutterPointerButtonMouseSecondary
            }
            FlutterPointerMouseButtons::Middle => {
                flutter_engine_sys::FlutterPointerMouseButtons::kFlutterPointerButtonMouseMiddle
            }
            FlutterPointerMouseButtons::Back => {
                flutter_engine_sys::FlutterPointerMouseButtons::kFlutterPointerButtonMouseBack
            }
            FlutterPointerMouseButtons::Forward => {
                flutter_engine_sys::FlutterPointerMouseButtons::kFlutterPointerButtonMouseForward
            }
        }
    }
}
