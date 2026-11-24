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
            // With a compositor the real presentation happens per view in
            // `compositor_present_view`; the engine still invokes the renderer's
            // `present` callback (Skia's `GLContextPresent`), and swapping the
            // wrong window's buffers there would be a bug. No-op it.
            if engine.compositor_handler.is_some() {
                true
            } else {
                engine.opengl_handler.swap_buffers()
            }
        })
    }
}

/// Present a whole software-rendered surface.
///
/// Only used without a compositor. flutter-glfw always installs one, so the
/// engine presents each view through the compositor handler instead; the
/// callback exists because the engine refuses a software renderer without it.
///
/// # Safety
///
/// Called by the engine with pointers it owns; they are ignored here.
pub unsafe extern "C" fn software_surface_present(
    _user_data: *mut c_void,
    _allocation: *const c_void,
    _row_bytes: usize,
    _height: usize,
) -> bool {
    true
}

/// The engine validates the Metal drawable callbacks even when a compositor
/// supplies every texture, so this stub keeps the configuration valid. With a
/// compositor the engine renders into the embedder's backing stores and never
/// asks for a drawable.
///
/// # Safety
///
/// Called by the engine only; the frame info is owned by the engine and unused.
pub unsafe extern "C" fn metal_get_next_drawable(
    _user_data: *mut c_void,
    _frame_info: *const flutter_engine_sys::FlutterFrameInfo,
) -> flutter_engine_sys::FlutterMetalTexture {
    flutter_engine_sys::FlutterMetalTexture {
        struct_size: std::mem::size_of::<flutter_engine_sys::FlutterMetalTexture>(),
        texture_id: 0,
        texture: std::ptr::null(),
        user_data: std::ptr::null_mut(),
        destruction_callback: None,
    }
}

/// See [`metal_get_next_drawable`]: present is never called with a compositor.
///
/// # Safety
///
/// Called by the engine only; the texture is owned by the engine and unused.
pub unsafe extern "C" fn metal_present_drawable(
    _user_data: *mut c_void,
    _texture: *const flutter_engine_sys::FlutterMetalTexture,
) -> bool {
    true
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

/// Allocate a per-view backing store through the configured compositor.
pub extern "C" fn compositor_create_backing_store(
    config: *const flutter_engine_sys::FlutterBackingStoreConfig,
    backing_store_out: *mut flutter_engine_sys::FlutterBackingStore,
    user_data: *mut c_void,
) -> bool {
    trace!("compositor_create_backing_store");
    if config.is_null() || backing_store_out.is_null() {
        return false;
    }
    // SAFETY: the engine owns both structs for the duration of this callback;
    // `user_data` is the leaked engine weak reference (see `present`).
    unsafe {
        with_engine(
            user_data,
            "compositor_create_backing_store",
            false,
            |engine| {
                let Some(handler) = engine.compositor_handler.as_ref() else {
                    return false;
                };
                let config = &*config;
                let size = (
                    config.size.width.max(0.0) as usize,
                    config.size.height.max(0.0) as usize,
                );
                handler.create_backing_store(config.view_id, size, &mut *backing_store_out)
            },
        )
    }
}

/// Release a backing store created by [`compositor_create_backing_store`].
pub extern "C" fn compositor_collect_backing_store(
    backing_store: *const flutter_engine_sys::FlutterBackingStore,
    user_data: *mut c_void,
) -> bool {
    trace!("compositor_collect_backing_store");
    if backing_store.is_null() {
        return false;
    }
    // SAFETY: see `compositor_create_backing_store`.
    unsafe {
        with_engine(
            user_data,
            "compositor_collect_backing_store",
            false,
            |engine| {
                let Some(handler) = engine.compositor_handler.as_ref() else {
                    return false;
                };
                handler.collect_backing_store(&*backing_store);
                true
            },
        )
    }
}

/// Composite one view's layers onto its native target.
pub extern "C" fn compositor_present_view(
    info: *const flutter_engine_sys::FlutterPresentViewInfo,
) -> bool {
    trace!("compositor_present_view");
    if info.is_null() {
        return false;
    }
    // SAFETY: the engine owns `info` and the layer array for the duration of
    // this callback, and `info.user_data` is the leaked engine weak reference.
    unsafe {
        let info = &*info;
        with_engine(info.user_data, "compositor_present_view", false, |engine| {
            let Some(handler) = engine.compositor_handler.as_ref() else {
                return false;
            };
            let layers: Vec<&flutter_engine_sys::FlutterLayer> =
                if info.layers.is_null() || info.layers_count == 0 {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(info.layers, info.layers_count)
                        .iter()
                        .map(|layer| &**layer)
                        .collect()
                };
            handler.present_view(info.view_id, &layers)
        })
    }
}

/// The framework asked the embedder to move native focus to a view.
pub extern "C" fn view_focus_change_request(
    request: *const flutter_engine_sys::FlutterViewFocusChangeRequest,
    user_data: *mut c_void,
) {
    trace!("view_focus_change_request");
    if request.is_null() {
        return;
    }
    // SAFETY: the engine owns `request` for the duration of the callback, and
    // `user_data` is the leaked engine weak reference (see `present`).
    unsafe {
        let request = &*request;
        with_engine(user_data, "view_focus_change_request", (), |engine| {
            let Some(handler) = engine.view_focus_change_handler.as_ref() else {
                return;
            };
            handler(
                request.view_id,
                request.state == flutter_engine_sys::FlutterViewFocusState::kFocused,
            );
        });
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
        assert!(!compositor_create_backing_store(
            ptr::null(),
            ptr::null_mut(),
            engine
        ));
        assert!(!compositor_collect_backing_store(ptr::null(), engine));
        assert!(!compositor_present_view(ptr::null()));
        view_focus_change_request(ptr::null(), engine);
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
