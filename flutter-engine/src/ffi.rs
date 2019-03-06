//! ffi interface based on libflutter.h

use libc::{c_int, size_t, c_char, c_void, uint8_t};
use std::{ffi::CString};
use utils::CStringVec;

#[repr(C)]
#[derive(PartialEq, Debug)]
pub enum FlutterResult {
    Success,
    InvalidLibraryVersion,
    InvalidArguments,
}

#[repr(C)]
#[derive(Debug)]
pub enum FlutterRendererType {
    OpenGL,
}

pub enum FlutterEngine {}

pub type VoidCallback = extern fn(*const c_void);
pub type BoolCallback = extern fn(*const c_void) -> bool;
pub type UIntCallback = extern fn(*const c_void) -> u32;

#[repr(C)]
pub struct FlutterOpenGLTexture{
    target: u32,
    name: u32,
    format: u32,
    user_data: *const c_void,
    destruction_callback: VoidCallback,
}

#[repr(C)]
pub struct FlutterTransformation {
  scale_x: f64,
  skew_x: f64,
  trans_x: f64,
  skew_y: f64,
  scale_y: f64,
  trans_y: f64,
  pers0: f64,
  pers1: f64,
  pers2: f64,
}


pub type TransformationCallback = extern fn (*const c_void) -> FlutterTransformation;
pub type ProcResolver = extern fn (
    *const c_void,
    *const c_char) -> *const c_void;
pub type TextureFrameCallback = extern fn (*const c_void,
                                     libc::int64_t /* texture identifier */,
                                     libc::size_t /* width */,
                                     libc::size_t /* height */,
                                     *const FlutterOpenGLTexture /* texture out */) -> bool;

#[repr(C)]
#[derive(Debug)]
pub struct FlutterOpenGLRendererConfig {
    pub struct_size: size_t,
    pub make_current: BoolCallback,
    pub clear_current: BoolCallback,
    pub present: BoolCallback,
    pub fbo_callback: UIntCallback,
    pub make_resource_current: BoolCallback,
    pub fbo_reset_after_present: bool,
    pub surface_transformation: Option<TransformationCallback>, 
    pub gl_proc_resolver: ProcResolver,
    pub gl_external_texture_frame_callback: Option<TextureFrameCallback>,
}

// TODO: Use union types when rust ffi support unnamed union field
//  https://github.com/rust-lang/rust/issues/49804

#[repr(C)]
#[derive(Debug)]
pub struct FlutterRendererConfig {
    pub kind: FlutterRendererType,
    pub open_gl: FlutterOpenGLRendererConfig,
}

#[repr(C)]
pub struct FlutterWindowMetricsEvent {
    pub struct_size: size_t,
    pub width: size_t,
    pub height: size_t,
    pub pixel_ratio: f64,
}

#[repr(C)]
pub enum FlutterPointerPhase {
    Cancel,
    Up,
    Down,
    Move,
}

#[repr(C)]
pub struct FlutterPointerEvent {
    pub struct_size: size_t,
    pub phase: FlutterPointerPhase,
    pub timestamp: size_t,  // in microseconds.
    pub x: f64,
    pub y: f64,
}

#[derive(Debug)]
pub enum FlutterPlatformMessageResponseHandle {}

#[repr(C)]
#[derive(Debug)]
pub struct FlutterPlatformMessage {
    pub struct_size: size_t,
    pub channel: *const c_char,
    pub message: *const uint8_t,
    pub message_size: size_t,
    pub response_handle: *const FlutterPlatformMessageResponseHandle,
}

impl FlutterPlatformMessage {
    /// This method is called manually. Message from C is managed by flutter.
    /// But message back to flutter is managed by me.
    pub fn drop(&mut self) {
        unsafe {
            let _ = CString::from_raw(self.channel as *mut c_char);
            let _ = String::from_raw_parts(self.message as *mut u8, self.message_size, self.message_size);
        }
    }
}

pub type FlutterPlatformMessageCallback = extern fn(*const FlutterPlatformMessage, *const c_void);

#[repr(C)]
#[derive(Debug)]
pub struct FlutterProjectArgs {
    pub struct_size: size_t,
    pub assets_path: *mut c_char,
    pub main_path: *mut c_char,
    pub packages_path: *mut c_char,
    pub icu_data_path: *mut c_char,
    pub command_line_argc: c_int,
    pub command_line_argv: *mut *mut c_char,
    pub platform_message_callback: FlutterPlatformMessageCallback,
    pub vm_snapshot_data: *const u8,
    pub vm_snapshot_data_size: size_t,
    pub vm_snapshot_instructions: *const u8,
    pub vm_snapshot_instructions_size: size_t,
    pub isolate_snapshot_data: *const u8,
    pub isolate_snapshot_data_size: size_t,
    pub isolate_snapshot_instructions: *const u8,
    pub isolate_snapshot_instructions_size: size_t,
    pub root_isolate_create_callback: VoidCallback,
}

impl Drop for FlutterProjectArgs {
    fn drop(&mut self) {
        unsafe {
            let _ = CString::from_raw(self.assets_path);
            let _ = CString::from_raw(self.main_path);
            let _ = CString::from_raw(self.packages_path);
            let _ = CString::from_raw(self.icu_data_path);
            let _ = CStringVec::from_raw(self.command_line_argc as usize, self.command_line_argv);
        }
    }
}

#[cfg(target_os = "linux")]
#[link(name = "flutter_engine")]
extern {}

#[cfg(target_os = "macos")]
#[link(name = "FlutterEmbedder", kind = "framework")]
extern {}

#[cfg(target_os = "windows")]
#[link(name = "flutter_engine.dll")]
extern {}

extern "C" {
    pub fn FlutterEngineRun(
        version: size_t,
        config: *const FlutterRendererConfig,
        args: *const FlutterProjectArgs,
        user_data: *const c_void,
        engine_out: *const *const FlutterEngine) -> FlutterResult;

    pub fn FlutterEngineShutdown(
        engine: *const FlutterEngine) -> FlutterResult;

    pub fn FlutterEngineSendWindowMetricsEvent(
        engine: *const FlutterEngine,
        event: *const FlutterWindowMetricsEvent) -> FlutterResult;

    pub fn FlutterEngineSendPointerEvent(
        engine: *const FlutterEngine,
        event: *const FlutterPointerEvent,
        events_count: size_t) -> FlutterResult;

    pub fn FlutterEngineSendPlatformMessage(
        engine: *const FlutterEngine,
        event: *const FlutterPlatformMessage,
        ) -> FlutterResult;

    pub fn FlutterEngineSendPlatformMessageResponse(
        engine: *const FlutterEngine,
        handle: *const FlutterPlatformMessageResponseHandle,
        data: *const uint8_t,
        data_length: size_t,
        ) -> FlutterResult;

    pub fn __FlutterEngineFlushPendingTasksNow();
}