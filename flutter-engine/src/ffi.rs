//! Internal wrappers around some of the types/functions in [`flutter_engine_sys`]

use std::{
    borrow::Cow,
    ffi::{CStr, CString},
    mem, ptr,
    time::{SystemTime, UNIX_EPOCH},
};

use flutter_engine_sys::{FlutterPlatformMessage, FlutterPlatformMessageResponseHandle};
use log::{error, trace};

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

#[derive(Debug)]
pub struct FlutterEngine {
    engine_ptr: flutter_engine_sys::FlutterEngine,
}

unsafe impl Send for FlutterEngine {}
unsafe impl Sync for FlutterEngine {}

impl FlutterEngine {
    pub fn new(engine_ptr: flutter_engine_sys::FlutterEngine) -> Option<Self> {
        if engine_ptr.is_null() {
            None
        } else {
            Some(Self { engine_ptr })
        }
    }

    pub fn send_window_metrics_event(&self, width: i32, height: i32, pixel_ratio: f64) {
        let event = flutter_engine_sys::FlutterWindowMetricsEvent {
            struct_size: std::mem::size_of::<flutter_engine_sys::FlutterWindowMetricsEvent>(),
            width: width as usize,
            height: height as usize,
            pixel_ratio,
        };
        unsafe {
            flutter_engine_sys::FlutterEngineSendWindowMetricsEvent(self.engine_ptr, &event);
        }
    }

    pub fn send_pointer_event(
        &self,
        phase: FlutterPointerPhase,
        x: f64,
        y: f64,
        signal_kind: FlutterPointerSignalKind,
        scroll_delta_x: f64,
        scroll_delta_y: f64,
    ) {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let event = flutter_engine_sys::FlutterPointerEvent {
            struct_size: mem::size_of::<flutter_engine_sys::FlutterPointerEvent>(),
            timestamp: timestamp.as_micros() as usize,
            phase: phase.into(),
            x,
            y,
            device: 0,
            signal_kind: signal_kind.into(),
            scroll_delta_x,
            scroll_delta_y,
        };
        unsafe {
            flutter_engine_sys::FlutterEngineSendPointerEvent(self.engine_ptr, &event, 1);
        }
    }

    pub fn send_platform_message(&self, message: PlatformMessage) {
        trace!("Sending message on channel {}", message.channel);
        unsafe {
            flutter_engine_sys::FlutterEngineSendPlatformMessage(self.engine_ptr, &message.into());
        }
    }

    pub fn send_platform_message_response(
        &self,
        response_handle: PlatformMessageResponseHandle,
        bytes: &[u8],
    ) {
        trace!("Sending message response");
        unsafe {
            flutter_engine_sys::FlutterEngineSendPlatformMessageResponse(
                self.engine_ptr,
                response_handle.into(),
                bytes.as_ptr(),
                bytes.len(),
            );
        }
    }

    pub fn shutdown(&self) {
        unsafe {
            flutter_engine_sys::FlutterEngineShutdown(self.engine_ptr);
        }
    }
}
