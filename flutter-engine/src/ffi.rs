//! Internal wrappers around some of the types/functions in [`flutter_engine_sys`]

use std::{
    borrow::Cow,
    ffi::{CStr, CString},
    mem, ptr,
};

use flutter_engine_sys::{FlutterPlatformMessage, FlutterPlatformMessageResponseHandle};
use log::trace;

#[derive(Debug, Copy, Clone)]
pub struct PlatformMessageResponseHandle {
    handle: *const FlutterPlatformMessageResponseHandle,
}

impl Into<PlatformMessageResponseHandle> for *const FlutterPlatformMessageResponseHandle {
    fn into(self) -> PlatformMessageResponseHandle {
        PlatformMessageResponseHandle { handle: self }
    }
}

impl From<PlatformMessageResponseHandle> for *const FlutterPlatformMessageResponseHandle {
    fn from(handle: PlatformMessageResponseHandle) -> Self {
        handle.handle
    }
}

#[derive(Debug)]
pub struct PlatformMessage<'a, 'b> {
    pub channel: Cow<'a, str>,
    pub message: &'b [u8],
    pub response_handle: Option<PlatformMessageResponseHandle>,
}

impl<'a, 'b> Into<FlutterPlatformMessage> for &PlatformMessage<'a, 'b> {
    fn into(self) -> FlutterPlatformMessage {
        let response_handle = if let Some(h) = self.response_handle {
            h.into()
        } else {
            ptr::null()
        };
        FlutterPlatformMessage {
            struct_size: mem::size_of::<FlutterPlatformMessage>(),
            channel: CString::new(&*self.channel).unwrap().into_raw(),
            message: self.message.as_ptr(),
            message_size: self.message.len(),
            response_handle,
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

#[derive(Copy, Clone, Debug)]
pub struct FlutterEngine {
    engine_ptr: flutter_engine_sys::FlutterEngine,
}

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

    pub fn send_platform_message(&self, message: &PlatformMessage) {
        trace!("Sending message on channel {}", message.channel);
        unsafe {
            flutter_engine_sys::FlutterEngineSendPlatformMessage(self.engine_ptr, &message.into());
        }
    }

    pub fn send_platform_message_response(
        &self,
        response_handle: &PlatformMessageResponseHandle,
        bytes: &[u8],
    ) {
        trace!("Sending message response");
        unsafe {
            flutter_engine_sys::FlutterEngineSendPlatformMessageResponse(
                self.engine_ptr,
                (*response_handle).into(),
                bytes.as_ptr(),
                bytes.len(),
            );
        }
    }

    pub fn shutdown(self) {
        unsafe {
            flutter_engine_sys::FlutterEngineShutdown(self.engine_ptr);
        }
    }
}
