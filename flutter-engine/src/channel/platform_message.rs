use crate::FlutterEngine;
use flutter_engine_sys::{FlutterPlatformMessage, FlutterPlatformMessageResponseHandle};
use log::error;
use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::{mem, ptr};

#[derive(Debug)]
pub struct PlatformMessageResponseHandle {
    handle: *const FlutterPlatformMessageResponseHandle,
}

unsafe impl Send for PlatformMessageResponseHandle {}

unsafe impl Sync for PlatformMessageResponseHandle {}

impl PlatformMessageResponseHandle {
    pub fn new<F>(engine: FlutterEngine, callback: F) -> Self
    where
        F: FnOnce(&[u8]) -> () + 'static + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            let mut handle: *mut FlutterPlatformMessageResponseHandle = ptr::null_mut();
            flutter_engine_sys::FlutterPlatformMessageCreateResponseHandle(
                engine.engine_ptr(),
                Some(response_handle_callback),
                Box::into_raw(Box::new(callback)) as _,
                &mut handle,
            );

            Self { handle }
        }
    }
}

type ResponseType = Box<dyn FnOnce(&[u8]) + Send>;

unsafe extern "C" fn response_handle_callback(
    data: *const u8,
    size: usize,
    user_data: *mut c_void,
) {
    log::trace!("response_handle_callback");
    let message = std::slice::from_raw_parts(data, size);

    let user_data = user_data as *mut ResponseType;
    let user_data = Box::from_raw(user_data);
    user_data(message);
}

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
