use libc::{c_char, c_uint, c_void};
use log::trace;
use crate::{FlutterEngineHandler, FlutterEngineInner};
use std::sync::Arc;
use parking_lot::Mutex;
use crate::tasks::{TaskRunnerInner, TaskRunner};

#[inline]
unsafe fn get_handler(user_data: *mut c_void) -> Option<Arc<dyn FlutterEngineHandler>> {
    let engine = &*(user_data as *const FlutterEngineInner);
    engine.handler.upgrade()
}

pub extern "C" fn present(user_data: *mut c_void) -> bool {
    trace!("present");
    unsafe {
        if let Some(handler) = get_handler(user_data) {
            handler.swap_buffers()
        } else {
            false
        }
    }
}

pub extern "C" fn make_current(user_data: *mut c_void) -> bool {
    trace!("make_current");
    unsafe {
        if let Some(handler) = get_handler(user_data) {
            handler.make_current()
        } else {
            false
        }
    }
}

pub extern "C" fn clear_current(user_data: *mut c_void) -> bool {
    trace!("clear_current");
    unsafe {
        if let Some(handler) = get_handler(user_data) {
            handler.clear_current()
        } else {
            false
        }
    }
}

pub extern "C" fn fbo_callback(user_data: *mut c_void) -> c_uint {
    trace!("fbo_callback");
    unsafe {
        if let Some(handler) = get_handler(user_data) {
            handler.fbo_callback()
        } else {
            0
        }
    }
}

pub extern "C" fn make_resource_current(user_data: *mut c_void) -> bool {
    trace!("make_resource_current");
    unsafe {
        if let Some(handler) = get_handler(user_data) {
            handler.make_resource_current()
        } else {
            false
        }
    }
}

pub extern "C" fn gl_proc_resolver(user_data: *mut c_void, proc: *const c_char) -> *mut c_void {
    trace!("gl_proc_resolver");
    unsafe {
        if let Some(handler) = get_handler(user_data) {
            handler.gl_proc_resolver(proc)
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
        let engine = &*(user_data as *const FlutterEngineInner);
        engine.plugins.write().handle((*platform_message).into());
    }
}

pub extern "C" fn root_isolate_create_callback(_user_data: *mut c_void) {
    trace!("root_isolate_create_callback");
    // // This callback is executed on the main thread
    // unsafe {
    //     let user_data = &mut *(user_data as *mut DesktopUserData);
    //     if let DesktopUserData::WindowState(window_state) = user_data {
    //         window_state.set_isolate_created();
    //     }
    // }
}

pub extern "C" fn runs_task_on_current_thread(user_data: *mut c_void) -> bool {
    trace!("runs_task_on_current_thread");
    unsafe {
        let inner = &*(user_data as *const Mutex<TaskRunnerInner>);
        inner.lock().runs_task_on_current_thread()
    }
}

pub extern "C" fn post_task(
    task: flutter_engine_sys::FlutterTask,
    target_time_nanos: u64,
    user_data: *mut c_void,
) {
    trace!("post_task");
    unsafe {
        let inner = &*(user_data as *const Mutex<TaskRunnerInner>);
        let mut inner = inner.lock();
        TaskRunner::post_task(&mut inner, task, target_time_nanos);
    }
}

pub extern "C" fn gl_external_texture_frame(
    user_data: *mut c_void,
    texture_id: i64,
    width: usize,
    height: usize,
    texture: *mut flutter_engine_sys::FlutterOpenGLTexture,
) -> bool {
    trace!("gl_external_texture_frame");
    unsafe {
        if let Some(handler) = get_handler(user_data) {
            if let Some(frame) = handler.get_texture_frame(texture_id, (width, height)) {
                frame.to_ffi(&mut *texture);
                return true;
            }
        }
        false
    }
}
