use super::DesktopUserData;

use glfw::Context;
use libc::{c_char, c_uint, c_void};
use log::trace;

pub extern "C" fn present(user_data: *mut c_void) -> bool {
    trace!("present");
    unsafe {
        let user_data = &*(user_data as *mut DesktopUserData);
        if let Some(window) = user_data.get_window() {
            window.borrow_mut().swap_buffers();
            true
        } else {
            false
        }
    }
}

pub extern "C" fn make_current(user_data: *mut c_void) -> bool {
    trace!("make_current");
    unsafe {
        let user_data = &*(user_data as *mut DesktopUserData);
        if let Some(window) = user_data.get_window() {
            window.borrow_mut().make_current();
            true
        } else {
            false
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
        let user_data = &*(user_data as *mut DesktopUserData);
        if let Some(window) = user_data.get_window() {
            window
                .borrow()
                .glfw
                .get_proc_address_raw(&glfw::string_from_c_str(proc)) as *mut c_void
        } else {
            std::ptr::null_mut()
        }
    }
}

pub extern "C" fn platform_message_callback(
    platform_message: *const flutter_engine_sys::FlutterPlatformMessage,
    user_data: *mut c_void,
) {
    trace!("platform_message_callback");
    unsafe {
        let user_data = &mut *(user_data as *mut DesktopUserData);
        if let DesktopUserData::WindowState(window_state) = user_data {
            window_state
                .plugin_registrar
                .handle((*platform_message).into());
        }
    }
}

pub extern "C" fn root_isolate_create_callback(user_data: *mut c_void) {
    trace!("root_isolate_create_callback");
}
