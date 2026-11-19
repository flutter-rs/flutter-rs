//! The C ABI the Dart windowing package calls.
//!
//! The table's address is handed to Dart as `PlatformDispatcher.engineId`
//! (flutter-engine forwards `FlutterProjectArgs.engine_id`). Dart casts that
//! integer to a pointer and calls the function pointers directly; no exported
//! symbols, no link flags and no platform-specific symbol conventions.
//!
//! Every function runs on the platform thread, from Dart's synchronous FFI call
//! inside the UI isolate. They never unwind: a panic is caught and reported as
//! `-1` (or `0` for booleans).

use crate::app::{AppState, EventCallback, FlutterApp, current_app};
use crate::window::{WindowKind, WindowMode, WindowRequest};
use parking_lot::Mutex;
use std::ffi::{CStr, c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::rc::Rc;
use std::sync::Arc;

/// `FgWindowParams::flags` bits.
pub(crate) const FLAG_HAS_SIZE: u32 = 1 << 0;
pub(crate) const FLAG_RESIZABLE: u32 = 1 << 1;
pub(crate) const FLAG_DECORATED: u32 = 1 << 2;
pub(crate) const FLAG_SHRINK_WRAP: u32 = 1 << 3;
pub(crate) const FLAG_HAS_POSITION: u32 = 1 << 4;
pub(crate) const FLAG_FLOATING: u32 = 1 << 5;
pub(crate) const FLAG_MODAL: u32 = 1 << 6;

/// Window kinds, matching `GlfwWindowKind` in Dart.
pub(crate) const KIND_REGULAR: i32 = 0;
pub(crate) const KIND_DIALOG: i32 = 1;
pub(crate) const KIND_TOOLTIP: i32 = 2;
pub(crate) const KIND_POPUP: i32 = 3;
pub(crate) const KIND_SATELLITE: i32 = 4;

/// Arguments for [`create_window`].
#[repr(C)]
pub struct FgWindowParams {
    /// One of the `KIND_*` constants.
    pub kind: i32,
    /// `FLAG_*` bits.
    pub flags: u32,
    /// Logical width.
    pub width: f64,
    /// Logical height.
    pub height: f64,
    /// Parent view id, or `-1`.
    pub parent: i64,
    /// Initial logical x position (used with `FLAG_HAS_POSITION`).
    pub x: f64,
    /// Initial logical y position (used with `FLAG_HAS_POSITION`).
    pub y: f64,
    /// NUL-terminated UTF-8 title.
    pub title: *const c_char,
}

type CreateWindowFn = unsafe extern "C" fn(*const FgWindowParams) -> i64;
type WindowIdFn = unsafe extern "C" fn(i64) -> i32;
type SetTitleFn = unsafe extern "C" fn(i64, *const c_char) -> i32;
type GetSizeFn = unsafe extern "C" fn(i64, *mut f64, *mut f64) -> i32;
type SetSizeFn = unsafe extern "C" fn(i64, f64, f64) -> i32;
type SetConstraintsFn = unsafe extern "C" fn(i64, f64, f64, f64, f64) -> i32;
type GetPositionFn = unsafe extern "C" fn(i64, *mut f64, *mut f64) -> i32;
type SetPositionFn = unsafe extern "C" fn(i64, f64, f64) -> i32;
type SetBoolFn = unsafe extern "C" fn(i64, i32) -> i32;
type GetBoolFn = unsafe extern "C" fn(i64) -> i32;
type GetScaleFn = unsafe extern "C" fn(i64) -> f64;
type WorkAreaFn = unsafe extern "C" fn(*mut f64, *mut f64, *mut f64, *mut f64) -> i32;
type SetEventCallbackFn = unsafe extern "C" fn(*mut c_void, *mut c_void) -> i32;
type SetParentFn = unsafe extern "C" fn(i64, i64) -> i32;
type UsesLogicalFn = unsafe extern "C" fn() -> i32;
type GetPhysicalSizeFn = unsafe extern "C" fn(i64, *mut f64, *mut f64) -> i32;
type SetPhysicalPositionFn = unsafe extern "C" fn(i64, f64, f64) -> i32;
type PhysicalWorkAreaFn = unsafe extern "C" fn(*mut f64, *mut f64, *mut f64, *mut f64) -> i32;

/// The function table. Its layout must match `_FgWindowingApi` in Dart.
#[repr(C)]
pub struct FgWindowingApi {
    /// ABI version; bump when a function's meaning changes.
    pub version: u32,
    /// `size_of::<FgWindowingApi>()`; lets Dart reject a layout mismatch.
    pub size: u32,
    pub create_window: CreateWindowFn,
    pub destroy_window: WindowIdFn,
    pub set_title: SetTitleFn,
    pub get_size: GetSizeFn,
    pub set_size: SetSizeFn,
    pub set_constraints: SetConstraintsFn,
    pub get_position: GetPositionFn,
    pub set_position: SetPositionFn,
    pub set_maximized: SetBoolFn,
    pub is_maximized: GetBoolFn,
    pub set_minimized: SetBoolFn,
    pub is_minimized: GetBoolFn,
    pub set_fullscreen: SetBoolFn,
    pub is_fullscreen: GetBoolFn,
    pub activate: WindowIdFn,
    pub is_activated: GetBoolFn,
    pub get_scale: GetScaleFn,
    pub get_monitor_workarea: WorkAreaFn,
    pub set_event_callback: SetEventCallbackFn,
    pub set_parent: SetParentFn,
    pub uses_logical_screen_coordinates: UsesLogicalFn,
    pub get_position_physical: GetPhysicalSizeFn,
    pub get_size_physical: GetPhysicalSizeFn,
    pub get_monitor_workarea_physical: PhysicalWorkAreaFn,
    pub set_position_physical: SetPhysicalPositionFn,
}

static API: FgWindowingApi = FgWindowingApi {
    version: 1,
    size: std::mem::size_of::<FgWindowingApi>() as u32,
    create_window,
    destroy_window,
    set_title,
    get_size,
    set_size,
    set_constraints,
    get_position,
    set_position,
    set_maximized,
    is_maximized,
    set_minimized,
    is_minimized,
    set_fullscreen,
    is_fullscreen,
    activate,
    is_activated,
    get_scale,
    get_monitor_workarea,
    set_event_callback,
    set_parent,
    uses_logical_screen_coordinates,
    get_position_physical,
    get_size_physical,
    get_monitor_workarea_physical,
    set_position_physical,
};

/// The table address, forwarded to Dart as `PlatformDispatcher.engineId`.
pub(crate) fn api_address() -> i64 {
    &API as *const FgWindowingApi as i64
}

fn with_app<R>(fallback: R, f: impl FnOnce(Rc<AppState>) -> R) -> R {
    let Some(state) = current_app() else {
        return fallback;
    };
    catch_unwind(AssertUnwindSafe(|| f(state))).unwrap_or(fallback)
}

fn with_window<R>(
    state: &AppState,
    view_id: i64,
    fallback: R,
    f: impl FnOnce(&mut glfw::PWindow) -> R,
) -> R {
    let window = state
        .windows
        .borrow()
        .get(&view_id)
        .map(|record| Arc::clone(&record.window));
    match window {
        Some(window) => {
            let mut window = window.lock();
            f(&mut window)
        }
        None => fallback,
    }
}

fn window_scale(state: &AppState, view_id: i64) -> f64 {
    state
        .windows
        .borrow()
        .get(&view_id)
        .map_or(1.0, |record| record.coordinate_scale())
}

fn title_from(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: the caller passes a NUL-terminated UTF-8 string for this call.
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// Create a native window and an engine view. Returns the view id or `-1`.
///
/// # Safety
///
/// `params` must point to a valid [`FgWindowParams`] and its `title` must be a
/// NUL-terminated string valid for this call.
pub unsafe extern "C" fn create_window(params: *const FgWindowParams) -> i64 {
    if params.is_null() {
        return -1;
    }
    // SAFETY: the caller guarantees the pointer.
    let params = unsafe { &*params };
    let kind = match params.kind {
        KIND_DIALOG => WindowKind::Dialog,
        KIND_TOOLTIP => WindowKind::Tooltip,
        KIND_POPUP => WindowKind::Popup,
        KIND_SATELLITE => WindowKind::Satellite,
        KIND_REGULAR => WindowKind::Regular,
        _ => WindowKind::Regular,
    };
    let (default_width, default_height) =
        if kind == WindowKind::Tooltip || kind == WindowKind::Popup {
            (240.0, 120.0)
        } else {
            (400.0, 300.0)
        };
    let request = WindowRequest {
        title: title_from(params.title),
        width: if params.flags & FLAG_HAS_SIZE != 0 {
            params.width.max(1.0)
        } else {
            default_width
        },
        height: if params.flags & FLAG_HAS_SIZE != 0 {
            params.height.max(1.0)
        } else {
            default_height
        },
        resizable: params.flags & FLAG_RESIZABLE != 0,
        decorated: params.flags & FLAG_DECORATED != 0 || kind.is_decorated(),
        floating: params.flags & FLAG_FLOATING != 0 || kind.is_floating(),
        parent: (params.parent >= 0).then_some(params.parent),
        modal: params.flags & FLAG_MODAL != 0,
        shrink_wrap: params.flags & FLAG_SHRINK_WRAP != 0,
        position: (params.flags & FLAG_HAS_POSITION != 0).then_some((params.x, params.y)),
        mode: WindowMode::Windowed,
    };

    with_app(-1, |state| {
        if !state.running.get() {
            log::error!("create_window: the app is not running yet");
            return -1;
        }
        let app = FlutterApp::from_state(state);
        match app.create_window_inner(request) {
            Ok(window) => window.view_id(),
            Err(error) => {
                log::error!("create_window failed: {error}");
                -1
            }
        }
    })
}

/// Destroy a window and remove its view.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn destroy_window(view_id: i64) -> i32 {
    with_app(-1, |state| {
        if !state.windows.borrow().contains_key(&view_id) {
            return -1;
        }
        state.destroy_window(view_id);
        0
    })
}

/// Set a window title.
///
/// # Safety
///
/// `title` must be a NUL-terminated string valid for this call.
pub unsafe extern "C" fn set_title(view_id: i64, title: *const c_char) -> i32 {
    let title = title_from(title);
    with_app(-1, |state| {
        with_window(&state, view_id, -1, |window| {
            window.set_title(&title);
            0
        })
    })
}

/// Read a window's logical size.
///
/// # Safety
///
/// `width`/`height` must be valid writable pointers.
pub unsafe extern "C" fn get_size(view_id: i64, width: *mut f64, height: *mut f64) -> i32 {
    if width.is_null() || height.is_null() {
        return -1;
    }
    with_app(-1, |state| {
        let size = state
            .windows
            .borrow()
            .get(&view_id)
            .map(|record| record.logical_size);
        match size {
            Some((w, h)) => {
                // SAFETY: the caller guarantees the pointers.
                unsafe {
                    *width = w;
                    *height = h;
                }
                0
            }
            None => -1,
        }
    })
}

/// Set a window's logical size.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn set_size(view_id: i64, width: f64, height: f64) -> i32 {
    with_app(-1, |state| {
        let scale = window_scale(&state, view_id);
        with_window(&state, view_id, -1, |window| {
            window.set_size(
                (width * scale).round() as i32,
                (height * scale).round() as i32,
            );
            0
        })
    })
}

/// Set a window's size limits. A value `<= 0` means unbounded.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn set_constraints(
    view_id: i64,
    min_width: f64,
    min_height: f64,
    max_width: f64,
    max_height: f64,
) -> i32 {
    fn bound(value: f64) -> Option<u32> {
        (value > 0.0).then_some(value.round() as u32)
    }
    with_app(-1, |state| {
        let scale = window_scale(&state, view_id);
        let scaled = |value: f64| bound(value * scale);
        with_window(&state, view_id, -1, |window| {
            window.set_size_limits(
                scaled(min_width),
                scaled(min_height),
                scaled(max_width),
                scaled(max_height),
            );
            0
        })
    })
}

/// Read a window's logical position.
///
/// # Safety
///
/// `x`/`y` must be valid writable pointers.
pub unsafe extern "C" fn get_position(view_id: i64, x: *mut f64, y: *mut f64) -> i32 {
    if x.is_null() || y.is_null() {
        return -1;
    }
    with_app(-1, |state| {
        let position = state
            .windows
            .borrow()
            .get(&view_id)
            .map(|record| (record.window.lock().get_pos(), record.coordinate_scale()));
        match position {
            Some(((px, py), scale)) => {
                // SAFETY: the caller guarantees the pointers.
                unsafe {
                    *x = f64::from(px) / scale;
                    *y = f64::from(py) / scale;
                }
                0
            }
            None => -1,
        }
    })
}

/// Set a window's logical position. Native Wayland ignores this (GLFW reports
/// `GLFW_PLATFORM_ERROR`); X11, Windows and macOS honour it.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn set_position(view_id: i64, x: f64, y: f64) -> i32 {
    with_app(-1, |state| {
        let scale = window_scale(&state, view_id);
        let target = ((x * scale).round() as i32, (y * scale).round() as i32);
        let result = with_window(&state, view_id, -1, |window| {
            window.set_pos(target.0, target.1);
            0
        });
        if result == 0 {
            state.note_requested_position(view_id, target);
        }
        result
    })
}

/// Maximize or restore a window.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn set_maximized(view_id: i64, maximized: i32) -> i32 {
    with_app(-1, |state| {
        with_window(&state, view_id, -1, |window| {
            if maximized != 0 {
                window.maximize();
            } else {
                window.restore();
            }
            0
        })
    })
}

/// Whether a window is maximized.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn is_maximized(view_id: i64) -> i32 {
    with_app(0, |state| {
        with_window(&state, view_id, 0, |window| {
            i32::from(window.is_maximized())
        })
    })
}

/// Iconify or restore a window.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn set_minimized(view_id: i64, minimized: i32) -> i32 {
    with_app(-1, |state| {
        with_window(&state, view_id, -1, |window| {
            if minimized != 0 {
                window.iconify();
            } else {
                window.restore();
            }
            0
        })
    })
}

/// Whether a window is iconified.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn is_minimized(view_id: i64) -> i32 {
    with_app(0, |state| {
        with_window(&state, view_id, 0, |window| {
            i32::from(window.is_iconified())
        })
    })
}

/// Enter or leave fullscreen on the primary monitor.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn set_fullscreen(view_id: i64, fullscreen: i32) -> i32 {
    with_app(-1, |state| {
        let geometry = {
            let windows = state.windows.borrow();
            let Some(record) = windows.get(&view_id) else {
                return -1;
            };
            let window = record.window.lock();
            let framebuffer = window.get_framebuffer_size();
            (framebuffer, record.logical_size)
        };
        let mut glfw = state.glfw.clone();
        let result = glfw.with_connected_monitors(|_, monitors| {
            let Some(monitor) = monitors.first() else {
                return -1;
            };
            let monitor: &glfw::Monitor = monitor;
            with_window(&state, view_id, -1, |window| {
                if fullscreen != 0 {
                    window.set_monitor(
                        glfw::WindowMode::FullScreen(monitor),
                        0,
                        0,
                        geometry.0.0.max(1) as u32,
                        geometry.0.1.max(1) as u32,
                        None,
                    );
                } else {
                    window.set_monitor(
                        glfw::WindowMode::Windowed,
                        100,
                        100,
                        geometry.1.0.max(1.0) as u32,
                        geometry.1.1.max(1.0) as u32,
                        None,
                    );
                }
                0
            })
        });
        if result == 0 {
            if let Some(record) = state.windows.borrow_mut().get_mut(&view_id) {
                record.fullscreen = fullscreen != 0;
            }
            state.emit_event(
                view_id,
                crate::app::event::FULLSCREEN_CHANGED,
                f64::from(fullscreen),
                0.0,
            );
        }
        result
    })
}

/// Whether a window is fullscreen (tracked by the embedder; GLFW has no query).
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn is_fullscreen(view_id: i64) -> i32 {
    with_app(0, |state| {
        i32::from(
            state
                .windows
                .borrow()
                .get(&view_id)
                .is_some_and(|record| record.fullscreen),
        )
    })
}

/// Focus a window.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn activate(view_id: i64) -> i32 {
    with_app(-1, |state| {
        with_window(&state, view_id, -1, |window| {
            window.focus();
            0
        })
    })
}

/// Whether a window is focused.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn is_activated(view_id: i64) -> i32 {
    with_app(0, |state| {
        with_window(&state, view_id, 0, |window| i32::from(window.is_focused()))
    })
}

/// The window's content scale.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn get_scale(view_id: i64) -> f64 {
    with_app(1.0, |state| {
        with_window(&state, view_id, 1.0, |window| {
            f64::from(window.get_content_scale().0.max(0.01))
        })
    })
}

/// The primary monitor's work area, in screen coordinates.
///
/// # Safety
///
/// The out-pointers must be valid writable pointers.
pub unsafe extern "C" fn get_monitor_workarea(
    x: *mut f64,
    y: *mut f64,
    width: *mut f64,
    height: *mut f64,
) -> i32 {
    if x.is_null() || y.is_null() || width.is_null() || height.is_null() {
        return -1;
    }
    with_app(-1, |state| {
        let mut glfw = state.glfw.clone();
        let area = glfw.with_connected_monitors(|_, monitors| {
            monitors
                .first()
                .map(|monitor| (monitor.get_workarea(), monitor.get_content_scale().0))
        });
        match area {
            Some(((ax, ay, aw, ah), scale)) => {
                let scale = state.coordinate_scale(f64::from(scale));
                // SAFETY: the caller guarantees the pointers.
                unsafe {
                    *x = f64::from(ax) / scale;
                    *y = f64::from(ay) / scale;
                    *width = f64::from(aw) / scale;
                    *height = f64::from(ah) / scale;
                }
                0
            }
            None => -1,
        }
    })
}

/// Install the Dart event callback. Pass a null `callback` to remove it.
///
/// # Safety
///
/// `callback` must be null or a valid Dart `NativeCallable.listener` function
/// pointer with the signature `(i64, i32, f64, f64, void*)`.
pub unsafe extern "C" fn set_event_callback(callback: *mut c_void, user_data: *mut c_void) -> i32 {
    with_app(-1, |state| {
        if callback.is_null() {
            state.event_callback.set(None);
            state.event_user_data.set(ptr::null_mut());
            return 0;
        }
        // SAFETY: the caller guarantees the function pointer type; function and
        // data pointers have the same representation on every desktop target.
        let callback: EventCallback = unsafe { std::mem::transmute(callback) };
        state.event_callback.set(Some(callback));
        state.event_user_data.set(user_data);
        0
    })
}

/// Change a satellite's parent without moving it.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn set_parent(view_id: i64, parent: i64) -> i32 {
    with_app(-1, |state| {
        let mut windows = state.windows.borrow_mut();
        match windows.get_mut(&view_id) {
            Some(record) => {
                record.parent = (parent >= 0).then_some(parent);
                0
            }
            None => -1,
        }
    })
}

/// Whether GLFW's screen coordinates are logical (macOS points, Wayland units).
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn uses_logical_screen_coordinates() -> i32 {
    with_app(1, |state| i32::from(state.logical_screen_coordinates))
}

/// Read a window's raw GLFW position (physical on X11/Windows).
///
/// # Safety
///
/// `x`/`y` must be valid writable pointers.
pub unsafe extern "C" fn get_position_physical(x: i64, out_x: *mut f64, out_y: *mut f64) -> i32 {
    get_physical_position(x, out_x, out_y)
}

/// Read a window's raw framebuffer size in pixels.
///
/// # Safety
///
/// `width`/`height` must be valid writable pointers.
pub unsafe extern "C" fn get_size_physical(view_id: i64, width: *mut f64, height: *mut f64) -> i32 {
    if width.is_null() || height.is_null() {
        return -1;
    }
    with_app(-1, |state| {
        let size = state
            .windows
            .borrow()
            .get(&view_id)
            .map(|record| record.window.lock().get_framebuffer_size());
        match size {
            Some((w, h)) => {
                // SAFETY: the caller guarantees the pointers.
                unsafe {
                    *width = f64::from(w);
                    *height = f64::from(h);
                }
                0
            }
            None => -1,
        }
    })
}

/// The primary monitor's work area in physical pixels.
///
/// # Safety
///
/// The out-pointers must be valid writable pointers.
pub unsafe extern "C" fn get_monitor_workarea_physical(
    x: *mut f64,
    y: *mut f64,
    width: *mut f64,
    height: *mut f64,
) -> i32 {
    if x.is_null() || y.is_null() || width.is_null() || height.is_null() {
        return -1;
    }
    with_app(-1, |state| {
        let mut glfw = state.glfw.clone();
        let area = glfw.with_connected_monitors(|_, monitors| {
            monitors.first().map(|monitor| monitor.get_workarea())
        });
        match area {
            Some((ax, ay, aw, ah)) => {
                // SAFETY: the caller guarantees the pointers.
                unsafe {
                    *x = f64::from(ax);
                    *y = f64::from(ay);
                    *width = f64::from(aw);
                    *height = f64::from(ah);
                }
                0
            }
            None => -1,
        }
    })
}

/// Set a window position in raw GLFW screen coordinates.
///
/// # Safety
///
/// Callable from the platform thread only; the ABI guarantees that.
pub unsafe extern "C" fn set_position_physical(view_id: i64, x: f64, y: f64) -> i32 {
    with_app(-1, |state| {
        let mapped = state
            .windows
            .borrow()
            .get(&view_id)
            .is_none_or(|record| record.mapped);
        if !mapped {
            if let Some(record) = state.windows.borrow_mut().get_mut(&view_id) {
                record.pending_physical_position = Some((x, y));
            }
            return 0;
        }
        let target = (x.round() as i32, y.round() as i32);
        let result = with_window(&state, view_id, -1, |window| {
            window.set_pos(target.0, target.1);
            0
        });
        if result == 0 {
            state.note_requested_position(view_id, target);
        }
        result
    })
}

fn get_physical_position(view_id: i64, out_x: *mut f64, out_y: *mut f64) -> i32 {
    if out_x.is_null() || out_y.is_null() {
        return -1;
    }
    with_app(-1, |state| {
        let position = state
            .windows
            .borrow()
            .get(&view_id)
            .map(|record| record.window.lock().get_pos());
        match position {
            Some((px, py)) => {
                // SAFETY: the caller guarantees the pointers.
                unsafe {
                    *out_x = f64::from(px);
                    *out_y = f64::from(py);
                }
                0
            }
            None => -1,
        }
    })
}

/// A pointer to the mutex-wrapped window, for the app's own use.
#[allow(dead_code)]
fn window_ptr(state: &AppState, view_id: i64) -> Option<Arc<Mutex<glfw::PWindow>>> {
    state
        .windows
        .borrow()
        .get(&view_id)
        .map(|record| Arc::clone(&record.window))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn abi_layout_matches_the_dart_package() {
        // These numbers are asserted by flutter-glfw/dart/test/api_layout_test.dart.
        assert_eq!(size_of::<FgWindowParams>(), 56);
        assert_eq!(size_of::<FgWindowingApi>(), 8 + 25 * size_of::<usize>());
    }
}
