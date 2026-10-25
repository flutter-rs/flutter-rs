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
        // platform thread from its own. `glfw::init` has run, because a
        // `FlutterWindow` cannot exist before it.
        unsafe {
            glfw::ffi::glfwPostEmptyEvent();
        }
    }
}

pub(crate) struct GlfwOpenGLHandler {
    // The engine makes these contexts current on its raster and IO threads, so
    // `FlutterOpenGLHandler` is shared by `&` across threads and the interior
    // mutability has to be thread-safe.
    render_ctx: Mutex<glfw::PRenderContext>,
    resource_ctx: Mutex<glfw::PRenderContext>,
}

impl GlfwOpenGLHandler {
    pub fn new(render_ctx: glfw::PRenderContext, resource_ctx: glfw::PRenderContext) -> Self {
        Self {
            render_ctx: Mutex::new(render_ctx),
            resource_ctx: Mutex::new(resource_ctx),
        }
    }
}

impl FlutterOpenGLHandler for GlfwOpenGLHandler {
    fn swap_buffers(&self) -> bool {
        self.render_ctx.lock().swap_buffers();
        true
    }

    fn make_current(&self) -> bool {
        self.render_ctx.lock().make_current();
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

/// Backs `flutter/platform`: the window title and the system clipboard.
pub struct GlfwPlatformHandler {
    pub window: Arc<Mutex<glfw::PWindow>>,
}

impl PlatformHandler for GlfwPlatformHandler {
    fn set_application_switcher_description(&mut self, description: AppSwitcherDescription) {
        self.window.lock().set_title(&description.label);
    }

    fn set_clipboard_data(&mut self, text: String) {
        self.window.lock().set_clipboard_string(&text);
    }

    fn get_clipboard_data(&mut self, mime: &str) -> Result<String, MimeError> {
        match mime {
            "text/plain" => Ok(match self.window.lock().get_clipboard_string() {
                None => "".to_string(),
                Some(val) => val,
            }),
            _ => Err(MimeError),
        }
    }

    fn exit_application(&mut self, exit_type: AppExitType, exit_code: i32) -> AppExitResponse {
        // Raising the close flag is what ends `FlutterWindow::run`, which
        // shuts the engine down on its way out. `exit_code` is only logged:
        // the host's `main` decides what the process returns once `run` has
        // returned, and this handler has no way to reach it.
        log::debug!("the framework asked to exit ({exit_type:?}, code {exit_code})");
        self.window.lock().set_should_close(true);

        AppExitResponse::Exit
    }
}

/// Backs `flutter/window`: window geometry, visibility and client-side
/// dragging of a borderless window.
pub struct GlfwWindowHandler {
    window: Arc<Mutex<glfw::PWindow>>,
    dragging: bool,
    start_cursor_pos: (f64, f64),
}

impl GlfwWindowHandler {
    pub fn new(window: Arc<Mutex<glfw::PWindow>>) -> Self {
        Self {
            window,
            dragging: false,
            start_cursor_pos: (0.0, 0.0),
        }
    }

    /// Move the window by the cursor's offset from where [`start_drag`] was
    /// called, and report whether a drag is in progress — in which case the
    /// cursor event belongs to the drag and not to Flutter.
    ///
    /// [`start_drag`]: flutter_plugins::window::WindowHandler::start_drag
    pub fn drag_window(&self, x: f64, y: f64) -> bool {
        if self.dragging {
            let mut window = self.window.lock();
            let (wx, wy) = window.get_pos();
            // `as i32` saturates, and GLFW cursor positions are far inside
            // i32; round rather than truncate so dragging left/up moves by the
            // same amount as dragging right/down.
            let dx = (x - self.start_cursor_pos.0).round() as i32;
            let dy = (y - self.start_cursor_pos.1).round() as i32;
            window.set_pos(wx + dx, wy + dy);
        }
        self.dragging
    }
}

impl WindowHandler for GlfwWindowHandler {
    fn close(&mut self) {
        self.window.lock().set_should_close(true);
    }

    fn show(&mut self) {
        self.window.lock().show();
    }

    fn hide(&mut self) {
        self.window.lock().hide();
    }

    fn maximize(&mut self) {
        self.window.lock().maximize();
    }

    fn iconify(&mut self) {
        self.window.lock().iconify();
    }

    fn restore(&mut self) {
        self.window.lock().restore();
    }

    fn is_maximized(&mut self) -> bool {
        self.window.lock().is_maximized()
    }

    fn is_iconified(&mut self) -> bool {
        self.window.lock().is_iconified()
    }

    fn is_visible(&mut self) -> bool {
        self.window.lock().is_visible()
    }

    fn set_pos(&mut self, pos: PositionParams) {
        self.window.lock().set_pos(pos.x as i32, pos.y as i32);
    }

    fn get_pos(&mut self) -> PositionParams {
        let (x, y) = self.window.lock().get_pos();
        PositionParams {
            x: x as f32,
            y: y as f32,
        }
    }

    fn start_drag(&mut self) {
        self.dragging = true;
        self.start_cursor_pos = self.window.lock().get_cursor_pos();
    }

    fn end_drag(&mut self) {
        self.dragging = false;
    }
}

/// Backs `flutter/mousecursor`, mapping Flutter's cursor names onto the
/// handful of standard shapes GLFW ships.
pub struct GlfwMouseCursorHandler {
    window: Arc<Mutex<glfw::PWindow>>,
}

impl GlfwMouseCursorHandler {
    pub fn new(window: Arc<Mutex<glfw::PWindow>>) -> Self {
        Self { window }
    }
}

impl MouseCursorHandler for GlfwMouseCursorHandler {
    fn activate_system_cursor(&mut self, kind: &str) {
        let mut window = self.window.lock();
        if kind == "none" {
            window.set_cursor_mode(glfw::CursorMode::Hidden);
            return;
        }
        window.set_cursor_mode(glfw::CursorMode::Normal);
        // GLFW only ships a handful of standard cursors; map the rest to the
        // closest available shape.
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
