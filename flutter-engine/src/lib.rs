mod codec;
mod desktop_window_state;
mod ffi;
mod flutter_callbacks;
mod plugins;
mod utils;

use self::desktop_window_state::DesktopWindowState;

use std::ffi::CString;

use log::{debug, error};

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Error {
    WindowAlreadyCreated,
    WindowCreationFailed,
    EngineFailed,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use std::error::Error;
        f.write_str(self.description())
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::EngineFailed => "Engine call failed",
            Error::WindowCreationFailed => "Failed to create a window",
            Error::WindowAlreadyCreated => "Window was already created",
        }
    }
}

pub struct FlutterEngine {
    glfw: glfw::Glfw,
    window_state: Option<DesktopWindowState>,
}

pub fn init() -> Result<FlutterEngine, glfw::InitError> {
    glfw::init(Some(glfw::Callback {
        f: glfw_error_callback,
        data: (),
    }))
    .map(|glfw| FlutterEngine {
        glfw,
        window_state: None,
    })
}

impl FlutterEngine {
    pub fn create_window(
        &mut self,
        width: i32,
        height: i32,
        assets_path: String,
        icu_data_path: String,
        arguments: Vec<String>,
    ) -> Result<(), Error> {
        if self.window_state.is_some() {
            return Err(Error::WindowAlreadyCreated);
        }
        let (window, receiver) = self
            .glfw
            .create_window(width as u32, height as u32, "", glfw::WindowMode::Windowed)
            .ok_or(Error::WindowCreationFailed)?;

        // TODO: clear window canvas

        // as FlutterEngineRun already calls the make_current callback, the window state must be set before calling run
        self.window_state = Some(DesktopWindowState::new(window, receiver));
        let engine = self.run_flutter_engine(assets_path, icu_data_path, arguments)?;
        // now set the engine
        let window_state = self.window_state.as_mut().unwrap();
        window_state.set_engine(engine);
        // send initial size callback to engine
        window_state.send_framebuffer_size_change(window_state.window.get_framebuffer_size());

        // enable event polling
        window_state.window.set_char_polling(true);
        window_state.window.set_cursor_pos_polling(true);
        window_state.window.set_cursor_enter_polling(true);
        window_state.window.set_framebuffer_size_polling(true);
        window_state.window.set_key_polling(true);
        window_state.window.set_mouse_button_polling(true);
        window_state.window.set_scroll_polling(true);
        window_state.window.set_size_polling(true);

        Ok(())
    }

    fn run_flutter_engine(
        &mut self,
        assets_path: String,
        icu_data_path: String,
        mut arguments: Vec<String>,
    ) -> Result<flutter_engine_sys::FlutterEngine, Error> {
        arguments.insert(0, "placeholder".into());
        let arguments = utils::CStringVec::new(&arguments);

        let renderer_config = flutter_engine_sys::FlutterRendererConfig {
            type_: flutter_engine_sys::FlutterRendererType::kOpenGL,
            __bindgen_anon_1: flutter_engine_sys::FlutterRendererConfig__bindgen_ty_1 {
                open_gl: flutter_engine_sys::FlutterOpenGLRendererConfig {
                    struct_size: std::mem::size_of::<flutter_engine_sys::FlutterOpenGLRendererConfig>(
                    ),
                    make_current: Some(flutter_callbacks::make_current),
                    clear_current: Some(flutter_callbacks::clear_current),
                    present: Some(flutter_callbacks::present),
                    fbo_callback: Some(flutter_callbacks::fbo_callback),
                    make_resource_current: Some(flutter_callbacks::make_resource_current),
                    fbo_reset_after_present: false,
                    surface_transformation: None,
                    gl_proc_resolver: Some(flutter_callbacks::gl_proc_resolver),
                    gl_external_texture_frame_callback: None,
                },
            },
        };
        let project_args = flutter_engine_sys::FlutterProjectArgs {
            struct_size: std::mem::size_of::<flutter_engine_sys::FlutterProjectArgs>(),
            assets_path: CString::new(assets_path).unwrap().into_raw(),
            main_path__unused__: std::ptr::null(),
            packages_path__unused__: std::ptr::null(),
            icu_data_path: CString::new(icu_data_path).unwrap().into_raw(),
            command_line_argc: arguments.len() as i32,
            command_line_argv: arguments.into_raw(),
            platform_message_callback: Some(flutter_callbacks::platform_message_callback),
            vm_snapshot_data: std::ptr::null(),
            vm_snapshot_data_size: 0,
            vm_snapshot_instructions: std::ptr::null(),
            vm_snapshot_instructions_size: 0,
            isolate_snapshot_data: std::ptr::null(),
            isolate_snapshot_data_size: 0,
            isolate_snapshot_instructions: std::ptr::null(),
            isolate_snapshot_instructions_size: 0,
            root_isolate_create_callback: Some(flutter_callbacks::root_isolate_create_callback),
            update_semantics_node_callback: None,
            update_semantics_custom_action_callback: None,
            persistent_cache_path: std::ptr::null(),
            is_persistent_cache_read_only: false,
            vsync_callback: None,
            custom_dart_entrypoint: std::ptr::null(),
            custom_task_runners: std::ptr::null(),
        };

        unsafe {
            let engine_ptr: flutter_engine_sys::FlutterEngine = std::ptr::null_mut();
            if flutter_engine_sys::FlutterEngineRun(
                1,
                &renderer_config,
                &project_args,
                self as *mut FlutterEngine as *mut std::ffi::c_void,
                &engine_ptr as *const flutter_engine_sys::FlutterEngine
                    as *mut flutter_engine_sys::FlutterEngine,
            ) != flutter_engine_sys::FlutterEngineResult::kSuccess
                || engine_ptr.is_null()
            {
                Err(Error::EngineFailed)
            } else {
                Ok(engine_ptr)
            }
        }
    }

    pub fn run_window_loop(mut self) {
        let window_state = self.window_state.unwrap();
        let engine = window_state.get_engine();
        let window = window_state.window;
        let events = window_state.window_event_receiver;
        if cfg!(target_os = "linux") {
            unsafe {
                x11::xlib::XInitThreads();
            }
        }

        while !window.should_close() {
            self.glfw.poll_events();
            self.glfw.wait_events_timeout(1.0 / 60.0);

            for (_, event) in glfw::flush_messages(&events) {
                debug!("{:?}", event);
            }

            unsafe {
                flutter_engine_sys::__FlutterEngineFlushPendingTasksNow();
            }
        }

        unsafe {
            flutter_engine_sys::FlutterEngineShutdown(engine);
        }
    }
}

fn glfw_error_callback(error: glfw::Error, description: String, _: &()) {
    error!("GLFW error: {}", description);
}
