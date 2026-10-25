//! The `extern "C"` callbacks the engine calls into.
//!
//! Three rules hold for every function in this file:
//!
//! 1. A panic must never reach the `extern "C"` boundary. Unwinding is not
//!    allowed across it, so the process would abort with `SIGABRT` — no Dart
//!    error, no unwinding, no destructors. Every body therefore runs inside
//!    [`catch_unwind`] and reports a failure value to the engine instead.
//! 2. `user_data` is a *weak* reference leaked in `FlutterEngine::new`, so every
//!    callback may find the engine already gone and must cope with it.
//! 3. The engine calls these from several of its own threads (platform, raster,
//!    IO), not only the platform thread.

use crate::FlutterEngineInner;
use crate::channel::platform_message::PlatformMessage;
use crate::tasks::{TaskRunner, TaskRunnerInner};
use log::{debug, error, trace};
use parking_lot::Mutex;
use std::mem::ManuallyDrop;
use std::os::raw::{c_char, c_uint, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::Weak;

/// Run `f` with the engine behind the leaked `Weak` in `user_data`, returning
/// `fallback` if the engine is gone or `f` panicked.
///
/// # Safety
///
/// `user_data` must be the pointer `Weak::into_raw` produced in
/// `FlutterEngine::new`. That weak reference is deliberately leaked, so the
/// allocation (though not necessarily the engine itself) stays alive for the
/// whole process and the pointer never dangles.
unsafe fn with_engine<T>(
    user_data: *mut c_void,
    what: &str,
    fallback: T,
    f: impl FnOnce(&FlutterEngineInner) -> T,
) -> T {
    // SAFETY: the caller guarantees `user_data` is the leaked
    // `Weak<FlutterEngineInner>`. `ManuallyDrop` keeps the leaked weak count
    // intact, so the next callback still finds a live weak reference.
    let weak = ManuallyDrop::new(unsafe { Weak::from_raw(user_data as *const FlutterEngineInner) });
    let Some(engine) = weak.upgrade() else {
        debug!("{what}: the engine is gone, ignoring the callback");
        return fallback;
    };
    // `AssertUnwindSafe` is required because the handlers reach `&mut`-able
    // state; a caught panic may leave it half-updated, which is why the engine
    // is told the call failed.
    catch_unwind(AssertUnwindSafe(|| f(&engine))).unwrap_or_else(|_| {
        error!("panic in the {what} callback");
        fallback
    })
}

/// Like [`with_engine`], but for the two task-runner callbacks, whose user data
/// is the task runner rather than the engine.
///
/// # Safety
///
/// `user_data` must be the pointer `Weak::into_raw` produced for the platform
/// task runner in `FlutterEngine::new`; see [`with_engine`].
unsafe fn with_task_runner<T>(
    user_data: *mut c_void,
    what: &str,
    fallback: T,
    f: impl FnOnce(&Mutex<TaskRunnerInner>) -> T,
) -> T {
    // SAFETY: as documented above; `ManuallyDrop` keeps the leaked weak count
    // intact for the next callback.
    let weak =
        ManuallyDrop::new(unsafe { Weak::from_raw(user_data as *const Mutex<TaskRunnerInner>) });
    let Some(runner) = weak.upgrade() else {
        debug!("{what}: the task runner is gone, ignoring the callback");
        return fallback;
    };
    catch_unwind(AssertUnwindSafe(|| f(&runner))).unwrap_or_else(|_| {
        error!("panic in the {what} callback");
        fallback
    })
}

pub extern "C" fn present(user_data: *mut c_void) -> bool {
    trace!("present");
    // SAFETY: `user_data` is the leaked `Weak<FlutterEngineInner>` from
    // `FlutterEngine::new`, handed back unchanged by the engine.
    unsafe {
        with_engine(user_data, "present", false, |engine| {
            engine.opengl_handler.swap_buffers()
        })
    }
}

pub extern "C" fn make_current(user_data: *mut c_void) -> bool {
    trace!("make_current");
    // SAFETY: see `present`.
    unsafe {
        with_engine(user_data, "make_current", false, |engine| {
            engine.opengl_handler.make_current()
        })
    }
}

pub extern "C" fn clear_current(user_data: *mut c_void) -> bool {
    trace!("clear_current");
    // SAFETY: see `present`.
    unsafe {
        with_engine(user_data, "clear_current", false, |engine| {
            engine.opengl_handler.clear_current()
        })
    }
}

pub extern "C" fn fbo_callback(user_data: *mut c_void) -> c_uint {
    trace!("fbo_callback");
    // SAFETY: see `present`.
    unsafe {
        with_engine(user_data, "fbo_callback", 0, |engine| {
            engine.opengl_handler.fbo_callback()
        })
    }
}

pub extern "C" fn make_resource_current(user_data: *mut c_void) -> bool {
    trace!("make_resource_current");
    // SAFETY: see `present`.
    unsafe {
        with_engine(user_data, "make_resource_current", false, |engine| {
            engine.opengl_handler.make_resource_current()
        })
    }
}

pub extern "C" fn gl_proc_resolver(user_data: *mut c_void, proc: *const c_char) -> *mut c_void {
    trace!("gl_proc_resolver");
    // SAFETY: see `present`. `proc` is a NUL-terminated string owned by the
    // engine and only read for the duration of the call.
    unsafe {
        with_engine(user_data, "gl_proc_resolver", ptr::null_mut(), |engine| {
            engine.opengl_handler.gl_proc_resolver(proc)
        })
    }
}

pub extern "C" fn platform_message_callback(
    platform_message: *const flutter_engine_sys::FlutterPlatformMessage,
    user_data: *mut c_void,
) {
    trace!("platform_message_callback");
    let dispatch = |engine: &FlutterEngineInner| {
        // SAFETY: the engine owns `platform_message` for the duration of this
        // call, which is also all the borrowed `PlatformMessage` is used for,
        // and it carries either a live, unanswered response handle or none.
        let message = unsafe { PlatformMessage::from_raw(&*platform_message) };
        let registry = engine.channel_registry.read();
        match registry.channel(&message.channel) {
            // The channel's `Arc` is cloned out and the read guard dropped
            // before the handler runs: handlers are user code that may register
            // or remove channels, and the `RwLock` is not reentrant.
            Some(channel) => {
                drop(registry);
                channel.handle_platform_message(message);
            }
            None => registry.reject(message),
        }
    };
    // SAFETY: see `present`.
    unsafe { with_engine(user_data, "platform_message_callback", (), dispatch) }
}

/// Nothing in this crate needs to know when the root isolate is created, so
/// this is registered only so the trace shows the engine reaching that point.
pub extern "C" fn root_isolate_create_callback(_user_data: *mut c_void) {
    trace!("root_isolate_create_callback");
}

pub extern "C" fn runs_task_on_current_thread(user_data: *mut c_void) -> bool {
    trace!("runs_task_on_current_thread");
    // SAFETY: `user_data` is the leaked `Weak<Mutex<TaskRunnerInner>>` from
    // `FlutterEngine::new`, handed back unchanged by the engine.
    unsafe {
        with_task_runner(user_data, "runs_task_on_current_thread", false, |runner| {
            runner.lock().runs_task_on_current_thread()
        })
    }
}

pub extern "C" fn post_task(
    task: flutter_engine_sys::FlutterTask,
    target_time_nanos: u64,
    user_data: *mut c_void,
) {
    trace!("post_task");
    // SAFETY: see `runs_task_on_current_thread`. `task` is passed by value and
    // only ever handed back to `FlutterEngineRunTask`.
    unsafe {
        with_task_runner(user_data, "post_task", (), |runner| {
            let mut inner = runner.lock();
            TaskRunner::post_task(&mut inner, task, target_time_nanos);
        })
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
    // SAFETY: see `present`. `texture` is a live, writable
    // `FlutterOpenGLTexture` out-parameter owned by the engine.
    unsafe {
        with_engine(
            user_data,
            "gl_external_texture_frame",
            false,
            |engine| match engine
                .texture_registry
                .get_texture_frame(texture_id, (width, height))
            {
                Some(frame) => {
                    frame.into_ffi(&mut *texture);
                    true
                }
                None => false,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// After the engine is dropped the engine's own threads may still call in.
    /// Every callback must then report failure instead of dereferencing the
    /// stale user data — this test would segfault if one of them regressed to
    /// an unconditional `&*(user_data as *const _)`.
    #[test]
    fn a_callback_on_a_dead_engine_reports_failure() {
        // `Weak::new()` has no allocation, so this is exactly the "engine is
        // already gone" case the engine can hand us, with nothing to leak.
        let engine = Weak::<FlutterEngineInner>::new().into_raw() as *mut c_void;

        assert!(!present(engine));
        assert!(!make_current(engine));
        assert!(!clear_current(engine));
        assert_eq!(fbo_callback(engine), 0);
        assert!(!make_resource_current(engine));
        assert!(gl_proc_resolver(engine, c"glClear".as_ptr()).is_null());
        assert!(
            !gl_external_texture_frame(engine, 1, 2, 2, ptr::null_mut()),
            "a dead engine must not touch the out-parameter"
        );
        platform_message_callback(ptr::null(), engine);
        root_isolate_create_callback(engine);
    }

    /// Same for the two task-runner callbacks, whose user data is the task
    /// runner rather than the engine.
    #[test]
    fn a_task_runner_callback_on_a_dead_engine_reports_failure() {
        let runner = Weak::<Mutex<TaskRunnerInner>>::new().into_raw() as *mut c_void;

        assert!(!runs_task_on_current_thread(runner));
        post_task(
            flutter_engine_sys::FlutterTask {
                runner: ptr::null_mut(),
                task: 1,
            },
            0,
            runner,
        );
    }
}
