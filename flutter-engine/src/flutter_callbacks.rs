use super::FlutterEngine;

use glfw::Context;
use libc::{c_char, c_uint, c_void};
use log::trace;

pub extern "C" fn present(user_data: *mut c_void) -> bool {
    trace!("present");
    unsafe {
        let engine = &mut *(user_data as *mut FlutterEngine);
        match &mut engine.window_state {
            None => false,
            Some(window_state) => {
                window_state.window.swap_buffers();
                true
            }
        }
    }
}

pub extern "C" fn make_current(user_data: *mut c_void) -> bool {
    trace!("make_current");
    unsafe {
        let engine = &mut *(user_data as *mut FlutterEngine);
        match &mut engine.window_state {
            None => false,
            Some(window_state) => {
                window_state.window.make_current();
                true
            }
        }
    }
}

pub extern "C" fn clear_current(user_data: *mut c_void) -> bool {
    trace!("clear_current");
    glfw::make_context_current(None);
    true
}

pub extern "C" fn fbo_callback(user_data: *mut c_void) -> c_uint {
    trace!("fbo_callback");
    0
}

pub extern "C" fn make_resource_current(user_data: *mut c_void) -> bool {
    trace!("make_resource_current");
    false
}

pub extern "C" fn gl_proc_resolver(user_data: *mut c_void, proc: *const c_char) -> *mut c_void {
    trace!("gl_proc_resolver");
    unsafe {
        let engine = &mut *(user_data as *mut FlutterEngine);
        engine
            .glfw
            .get_proc_address_raw(&glfw::string_from_c_str(proc)) as *mut c_void
    }
}

pub extern "C" fn platform_message_callback(
    platform_message: *const flutter_engine_sys::FlutterPlatformMessage,
    user_data: *mut c_void,
) {
    trace!("platform_message_callback");
}

pub extern "C" fn root_isolate_create_callback(user_data: *mut c_void) {
    trace!("root_isolate_create_callback");
}
