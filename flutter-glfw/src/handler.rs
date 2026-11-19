use flutter_engine::FlutterOpenGLHandler;
use flutter_engine::tasks::TaskRunnerHandler;
use flutter_plugins::mousecursor::MouseCursorHandler;
use flutter_plugins::platform::{
    AppExitResponse, AppExitType, AppSwitcherDescription, MimeError, PlatformHandler,
};
use flutter_plugins::textinput::TextInputHandler;
use flutter_plugins::window::{PositionParams, WindowHandler};
use glfw::Context;
use parking_lot::Mutex;
use std::ffi::{c_char, c_void};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

pub(crate) struct GlfwPlatformTaskHandler {}

impl GlfwPlatformTaskHandler {
    pub fn new() -> Self {
        Self {}
    }
}

impl TaskRunnerHandler for GlfwPlatformTaskHandler {
    fn wake(&self) {
        // SAFETY: `glfwPostEmptyEvent` takes no arguments and GLFW documents it
        // as callable from any thread, which is the point: the engine wakes the
        // platform thread from its own. `glfw::init` has run, because an app
        // cannot exist before it.
        unsafe {
            glfw::ffi::glfwPostEmptyEvent();
        }
    }
}

pub(crate) struct GlfwOpenGLHandler {
    // The engine makes these contexts current on its raster and IO threads, so
    // `FlutterOpenGLHandler` is shared by `&` across threads and the interior
    // mutability has to be thread-safe. They are shared with the compositor.
    master_ctx: Arc<Mutex<glfw::PRenderContext>>,
    resource_ctx: Arc<Mutex<glfw::PRenderContext>>,
}

impl GlfwOpenGLHandler {
    pub fn new(
        master_ctx: Arc<Mutex<glfw::PRenderContext>>,
        resource_ctx: Arc<Mutex<glfw::PRenderContext>>,
    ) -> Self {
        Self {
            master_ctx,
            resource_ctx,
        }
    }
}

impl FlutterOpenGLHandler for GlfwOpenGLHandler {
    fn swap_buffers(&self) -> bool {
        // With a compositor, presentation happens per view in
        // `GlfwCompositor::present_view`; the engine still invokes this
        // callback (Skia's `GLContextPresent`), and swapping the master window
        // there would be wrong.
        true
    }

    fn make_current(&self) -> bool {
        self.master_ctx.lock().make_current();
        true
    }

    fn clear_current(&self) -> bool {
        glfw::make_context_current(None);
        true
    }

    fn fbo_callback(&self) -> u32 {
        0
    }

    fn make_resource_current(&self) -> bool {
        self.resource_ctx.lock().make_current();
        true
    }

    fn gl_proc_resolver(&self, proc: *const c_char) -> *mut c_void {
        // SAFETY: `proc` is the NUL-terminated symbol name the engine passes
        // us, which `glfwGetProcAddress` only reads. The engine calls this from
        // its raster thread, which has already made a context current through
        // `make_current`, as GLFW requires.
        unsafe { glfw::ffi::glfwGetProcAddress(proc as _) }
            .map_or(std::ptr::null_mut(), |f| f as *mut c_void)
    }
}

/// The window plugins apply to, and the borderless drag state.
///
/// A desktop app has one focused window at a time. Keeping the last focused
/// window here lets `flutter/platform`, `flutter-rs/window` and
/// `flutter/mousecursor` target it without knowing about the window map.
pub(crate) struct FocusedWindow {
    window: Mutex<Option<Arc<Mutex<glfw::PWindow>>>>,
    view_id: AtomicI64,
    dragging: AtomicBool,
    drag_view: AtomicI64,
    drag_start: Mutex<(f64, f64)>,
}

impl FocusedWindow {
    pub(crate) fn new() -> Self {
        Self {
            window: Mutex::new(None),
            view_id: AtomicI64::new(-1),
            dragging: AtomicBool::new(false),
            drag_view: AtomicI64::new(-1),
            drag_start: Mutex::new((0.0, 0.0)),
        }
    }

    pub(crate) fn set(&self, view_id: i64, window: Arc<Mutex<glfw::PWindow>>) {
        self.view_id.store(view_id, Ordering::Relaxed);
        *self.window.lock() = Some(window);
    }

    /// The last focused window, if any.
    pub(crate) fn get(&self) -> Option<Arc<Mutex<glfw::PWindow>>> {
        self.window.lock().clone()
    }

    pub(crate) fn clear(&self, view_id: i64) {
        if self.view_id.load(Ordering::Relaxed) == view_id {
            self.clear_all();
        }
    }

    /// Drop the retained window, for example on shutdown.
    pub(crate) fn clear_all(&self) {
        self.view_id.store(-1, Ordering::Relaxed);
        *self.window.lock() = None;
        self.dragging.store(false, Ordering::Relaxed);
        self.drag_view.store(-1, Ordering::Relaxed);
    }

    fn with_window<R>(&self, f: impl FnOnce(&mut glfw::PWindow) -> R) -> Option<R> {
        let window = self.window.lock().clone();
        window.map(|window| f(&mut window.lock()))
    }

    /// Move the window being dragged, if `view_id` owns the drag.
    ///
    /// Returns whether the cursor event belonged to the drag and must not be
    /// forwarded to Flutter.
    pub(crate) fn drag_window(&self, view_id: i64, x: f64, y: f64) -> bool {
        if !self.dragging.load(Ordering::Relaxed)
            || self.drag_view.load(Ordering::Relaxed) != view_id
        {
            return false;
        }
        let start = *self.drag_start.lock();
        self.with_window(|window| {
            let (wx, wy) = window.get_pos();
            // `as i32` saturates, and GLFW cursor positions are far inside
            // i32; round rather than truncate so dragging left/up moves by the
            // same amount as dragging right/down.
            let dx = (x - start.0).round() as i32;
            let dy = (y - start.1).round() as i32;
            window.set_pos(wx + dx, wy + dy);
        });
        true
    }
}

/// Backs `flutter/platform`: the window title, the system clipboard and exit.
pub struct GlfwPlatformHandler {
    pub focused: Arc<FocusedWindow>,
    pub quit: Arc<AtomicBool>,
}

impl PlatformHandler for GlfwPlatformHandler {
    fn set_application_switcher_description(&mut self, description: AppSwitcherDescription) {
        // The framework sends an empty label until the app sets a title; do not
        // let that clear the title the window was created with.
        if description.label.is_empty() {
            return;
        }
        self.focused
            .with_window(|window| window.set_title(&description.label));
    }

    fn set_clipboard_data(&mut self, text: String) {
        self.focused
            .with_window(|window| window.set_clipboard_string(&text));
    }

    fn get_clipboard_data(&mut self, mime: &str) -> Result<String, MimeError> {
        match mime {
            "text/plain" => Ok(self
                .focused
                .with_window(|window| window.get_clipboard_string())
                .flatten()
                .unwrap_or_default()),
            _ => Err(MimeError),
        }
    }

    fn exit_application(&mut self, exit_type: AppExitType, exit_code: i32) -> AppExitResponse {
        log::debug!("the framework asked to exit ({exit_type:?}, code {exit_code})");
        // The run loop closes every window and returns; the host's `main`
        // decides what the process returns.
        self.quit.store(true, Ordering::Relaxed);
        AppExitResponse::Exit
    }
}

/// Backs `flutter/window`: geometry, visibility and client-side dragging of a
/// borderless window. Every operation targets the focused window.
pub struct GlfwWindowHandler {
    focused: Arc<FocusedWindow>,
}

impl GlfwWindowHandler {
    pub fn new(focused: Arc<FocusedWindow>) -> Self {
        Self { focused }
    }
}

impl WindowHandler for GlfwWindowHandler {
    fn close(&mut self) {
        self.focused
            .with_window(|window| window.set_should_close(true));
    }

    fn show(&mut self) {
        self.focused.with_window(|window| window.show());
    }

    fn hide(&mut self) {
        self.focused.with_window(|window| window.hide());
    }

    fn maximize(&mut self) {
        self.focused.with_window(|window| window.maximize());
    }

    fn iconify(&mut self) {
        self.focused.with_window(|window| window.iconify());
    }

    fn restore(&mut self) {
        self.focused.with_window(|window| window.restore());
    }

    fn is_maximized(&mut self) -> bool {
        self.focused
            .with_window(|window| window.is_maximized())
            .unwrap_or(false)
    }

    fn is_iconified(&mut self) -> bool {
        self.focused
            .with_window(|window| window.is_iconified())
            .unwrap_or(true)
    }

    fn is_visible(&mut self) -> bool {
        self.focused
            .with_window(|window| window.is_visible())
            .unwrap_or(false)
    }

    fn set_pos(&mut self, pos: PositionParams) {
        self.focused
            .with_window(|window| window.set_pos(pos.x as i32, pos.y as i32));
    }

    fn get_pos(&mut self) -> PositionParams {
        self.focused
            .with_window(|window| {
                let (x, y) = window.get_pos();
                PositionParams {
                    x: x as f32,
                    y: y as f32,
                }
            })
            .unwrap_or(PositionParams { x: 0.0, y: 0.0 })
    }

    fn start_drag(&mut self) {
        let view_id = self.focused.view_id.load(Ordering::Relaxed);
        if view_id < 0 {
            return;
        }
        let start = self
            .focused
            .with_window(|window| window.get_cursor_pos())
            .unwrap_or((0.0, 0.0));
        *self.focused.drag_start.lock() = start;
        self.focused.drag_view.store(view_id, Ordering::Relaxed);
        self.focused.dragging.store(true, Ordering::Relaxed);
    }

    fn end_drag(&mut self) {
        self.focused.dragging.store(false, Ordering::Relaxed);
        self.focused.drag_view.store(-1, Ordering::Relaxed);
    }
}

/// Backs `flutter/mousecursor`, mapping Flutter's cursor names onto the
/// handful of standard shapes GLFW ships.
pub struct GlfwMouseCursorHandler {
    focused: Arc<FocusedWindow>,
}

impl GlfwMouseCursorHandler {
    pub fn new(focused: Arc<FocusedWindow>) -> Self {
        Self { focused }
    }
}

impl MouseCursorHandler for GlfwMouseCursorHandler {
    fn activate_system_cursor(&mut self, kind: &str) {
        self.focused.with_window(|window| {
            if kind == "none" {
                window.set_cursor_mode(glfw::CursorMode::Hidden);
                return;
            }
            window.set_cursor_mode(glfw::CursorMode::Normal);
            // GLFW only ships a handful of standard cursors; map the rest to
            // the closest available shape.
            let cursor = match kind {
                "click" | "grab" | "grabbing" => glfw::StandardCursor::Hand,
                "text" | "verticalText" => glfw::StandardCursor::IBeam,
                "precise" | "cell" => glfw::StandardCursor::Crosshair,
                "resizeLeftRight" | "resizeColumn" | "resizeLeft" | "resizeRight" => {
                    glfw::StandardCursor::HResize
                }
                "resizeUpDown" | "resizeRow" | "resizeUp" | "resizeDown" => {
                    glfw::StandardCursor::VResize
                }
                _ => glfw::StandardCursor::Arrow,
            };
            window.set_cursor(Some(glfw::Cursor::standard(cursor)));
        });
    }
}

/// Backs `flutter/textinput`'s show/hide requests, which a desktop window
/// with a hardware keyboard has nothing to do about.
#[derive(Default)]
pub struct GlfwTextInputHandler {}

impl TextInputHandler for GlfwTextInputHandler {
    fn show(&mut self) {}

    fn hide(&mut self) {}
}
