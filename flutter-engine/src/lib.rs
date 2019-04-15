mod channel;
mod codec;
mod desktop_window_state;
mod ffi;
mod flutter_callbacks;
mod plugins;
mod utils;

use crate::{desktop_window_state::DesktopWindowState, ffi::FlutterEngine};

use std::{cell::RefCell, ffi::CString, rc::Rc};

use log::{debug, error, trace};

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

enum DesktopUserData {
    None,
    Window(Rc<RefCell<glfw::Window>>),
    WindowState(DesktopWindowState),
}

impl DesktopUserData {
    pub fn get_window(&self) -> Option<&Rc<RefCell<glfw::Window>>> {
        match self {
            DesktopUserData::Window(window) => Some(window),
            DesktopUserData::WindowState(window_state) => Some(&window_state.runtime_data.window),
            DesktopUserData::None => None,
        }
    }
}

pub struct FlutterDesktop {
    glfw: glfw::Glfw,
    user_data: DesktopUserData,
}

pub fn init() -> Result<FlutterDesktop, glfw::InitError> {
    glfw::init(Some(glfw::Callback {
        f: glfw_error_callback,
        data: (),
    }))
    .map(|glfw| FlutterDesktop {
        glfw,
        user_data: DesktopUserData::None,
    })
}

impl FlutterDesktop {
    pub fn create_window(
        &mut self,
        width: i32,
        height: i32,
        assets_path: String,
        icu_data_path: String,
        arguments: Vec<String>,
    ) -> Result<(), Error> {
        match self.user_data {
            DesktopUserData::None => {}
            _ => return Err(Error::WindowAlreadyCreated),
        }
        let (window, receiver) = self
            .glfw
            .create_window(width as u32, height as u32, "", glfw::WindowMode::Windowed)
            .ok_or(Error::WindowCreationFailed)?;
        let window = Rc::new(RefCell::new(window));

        // TODO: clear window canvas

        // as FlutterEngineRun already calls the make_current callback, user_data must be set now
        self.user_data = DesktopUserData::Window(Rc::clone(&window));
        let engine = self.run_flutter_engine(assets_path, icu_data_path, arguments)?;
        // now create the full desktop state
        self.user_data =
            DesktopUserData::WindowState(DesktopWindowState::new(window, receiver, engine));

        if let DesktopUserData::WindowState(window_state) = &mut self.user_data {
            let framebuffer_size = window_state
                .runtime_data
                .window
                .borrow()
                .get_framebuffer_size();
            // send initial size callback to engine
            window_state.send_framebuffer_size_change(framebuffer_size);

            window_state
                .plugin_registrar
                .add_plugin(plugins::platform::PlatformPlugin::new());

            let mut window = window_state.runtime_data.window.borrow_mut();
            // enable event polling
            window.set_char_polling(true);
            window.set_cursor_pos_polling(true);
            window.set_cursor_enter_polling(true);
            window.set_framebuffer_size_polling(true);
            window.set_key_polling(true);
            window.set_mouse_button_polling(true);
            window.set_scroll_polling(true);
            window.set_size_polling(true);
        }

        Ok(())
    }

    fn run_flutter_engine(
        &mut self,
        assets_path: String,
        icu_data_path: String,
        mut arguments: Vec<String>,
    ) -> Result<FlutterEngine, Error> {
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
                &mut self.user_data as *mut DesktopUserData as *mut std::ffi::c_void,
                &engine_ptr as *const flutter_engine_sys::FlutterEngine
                    as *mut flutter_engine_sys::FlutterEngine,
            ) != flutter_engine_sys::FlutterEngineResult::kSuccess
                || engine_ptr.is_null()
            {
                Err(Error::EngineFailed)
            } else {
                Ok(FlutterEngine::new(engine_ptr).unwrap())
            }
        }
    }

    pub fn run_window_loop(mut self) {
        if let DesktopUserData::WindowState(window_state) = self.user_data {
            let events = &window_state.runtime_data.window_event_receiver;
            if cfg!(target_os = "linux") {
                unsafe {
                    x11::xlib::XInitThreads();
                }
            }

            while !window_state.runtime_data.window.borrow().should_close() {
                self.glfw.poll_events();
                self.glfw.wait_events_timeout(1.0 / 60.0);

                for (_, event) in glfw::flush_messages(&events) {
                    debug!("{:?}", event);
                }

                unsafe {
                    flutter_engine_sys::__FlutterEngineFlushPendingTasksNow();
                }
            }

            window_state.runtime_data.engine.shutdown();
        }
    }
}

fn glfw_error_callback(error: glfw::Error, description: String, _: &()) {
    error!("GLFW error: {}", description);
}
