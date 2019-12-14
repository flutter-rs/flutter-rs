use crate::texture_registry::TextureRegistry;
use flutter_engine::ffi::ExternalTextureFrame;
use flutter_engine::FlutterEngineHandler;
use glfw::Context;
use parking_lot::Mutex;
use std::ffi::c_void;
use std::sync::Arc;
use tokio::prelude::Future;
use tokio::runtime::TaskExecutor;

pub(crate) struct GlfwFlutterEngineHandler {
    pub(crate) glfw: glfw::Glfw,
    pub(crate) window: Arc<Mutex<glfw::Window>>,
    pub(crate) resource_window: Arc<Mutex<glfw::Window>>,
    pub(crate) task_executor: TaskExecutor,
    pub(crate) texture_registry: Arc<Mutex<TextureRegistry>>,
}

impl FlutterEngineHandler for GlfwFlutterEngineHandler {
    fn swap_buffers(&self) -> bool {
        self.window.lock().swap_buffers();
        true
    }

    fn make_current(&self) -> bool {
        self.window.lock().make_current();
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
        self.resource_window.lock().make_current();
        true
    }

    fn gl_proc_resolver(&self, proc: *const i8) -> *mut c_void {
        unsafe {
            self.glfw
                .get_proc_address_raw(&glfw::string_from_c_str(proc)) as *mut c_void
        }
    }

    fn wake_platform_thread(&self) {
        unsafe {
            glfw::ffi::glfwPostEmptyEvent();
        }
    }

    fn run_in_background(&self, func: Box<dyn FnOnce() + Send>) {
        self.task_executor
            .spawn(tokio::prelude::future::ok(()).map(move |_| {
                func();
            }));
    }

    fn get_texture_frame(
        &self,
        texture_id: i64,
        size: (usize, usize),
    ) -> Option<ExternalTextureFrame> {
        let (width, height) = size;
        self.texture_registry
            .lock()
            .get_texture_frame(texture_id, (width as u32, height as u32))
    }
}
