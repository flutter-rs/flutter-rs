#[macro_use]
mod macros;

pub mod channel;
pub mod codec;
mod desktop_window_state;
mod draw;
pub mod error;
mod ffi;
mod flutter_callbacks;
pub mod plugins;
mod utils;

pub use crate::desktop_window_state::{DesktopWindowState, InitData, RuntimeData};
use crate::ffi::FlutterEngine;
pub use crate::ffi::PlatformMessage;

use std::{cell::RefCell, ffi::CString, sync::mpsc::Receiver};

pub use glfw::{Context, Window};
use log::error;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Error {
    WindowAlreadyCreated,
    WindowCreationFailed,
    EngineFailed,
    MonitorNotFound,
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
            Error::MonitorNotFound => "No monitor with the specified index found",
        }
    }
}

pub enum WindowMode {
    Fullscreen(usize),
    Windowed,
    Borderless,
}

pub struct WindowArgs<'a> {
    pub width: i32,
    pub height: i32,
    pub title: &'a str,
    pub mode: WindowMode,
    pub bg_color: (u8, u8, u8),
}

enum DesktopUserData {
    None,
    Window(*mut glfw::Window, *mut glfw::Window),
    WindowState(DesktopWindowState),
}

impl DesktopUserData {
    pub fn get_window(&mut self) -> Option<&mut glfw::Window> {
        unsafe {
            match self {
                DesktopUserData::Window(window, _) => Some(&mut **window),
                DesktopUserData::WindowState(window_state) => Some(window_state.window()),
                DesktopUserData::None => None,
            }
        }
    }
    pub fn get_resource_window(&mut self) -> Option<&mut glfw::Window> {
        unsafe {
            match self {
                DesktopUserData::Window(_, window) => Some(&mut **window),
                DesktopUserData::WindowState(window_state) => Some(window_state.resource_window()),
                DesktopUserData::None => None,
            }
        }
    }
}

pub struct FlutterDesktop {
    glfw: glfw::Glfw,
    window: Option<glfw::Window>,
    resource_window: Option<glfw::Window>,
    resource_window_receiver: Option<Receiver<(f64, glfw::WindowEvent)>>,
    user_data: Box<RefCell<DesktopUserData>>,
}

pub fn init() -> Result<FlutterDesktop, glfw::InitError> {
    glfw::init(Some(glfw::Callback {
        f: glfw_error_callback,
        data: (),
    }))
    .map(|glfw| FlutterDesktop {
        glfw,
        window: None,
        resource_window: None,
        resource_window_receiver: None,
        user_data: Box::new(RefCell::new(DesktopUserData::None)),
    })
}

impl FlutterDesktop {
    pub fn create_window(
        &mut self,
        window_args: &WindowArgs,
        assets_path: String,
        icu_data_path: String,
        arguments: Vec<String>,
    ) -> Result<(), Error> {
        match *self.user_data.borrow() {
            DesktopUserData::None => {}
            _ => return Err(Error::WindowAlreadyCreated),
        }
        let (window, receiver) = match window_args.mode {
            WindowMode::Windowed => self
                .glfw
                .create_window(
                    window_args.width as u32,
                    window_args.height as u32,
                    window_args.title,
                    glfw::WindowMode::Windowed,
                )
                .ok_or(Error::WindowCreationFailed)?,
            WindowMode::Borderless => {
                self.glfw.window_hint(glfw::WindowHint::Decorated(false));
                self.glfw
                    .create_window(
                        window_args.width as u32,
                        window_args.height as u32,
                        window_args.title,
                        glfw::WindowMode::Windowed,
                    )
                    .ok_or(Error::WindowCreationFailed)?
            }
            WindowMode::Fullscreen(index) => {
                self.glfw
                    .with_connected_monitors(|glfw, monitors| -> Result<_, Error> {
                        let monitor = monitors.get(index).ok_or(Error::MonitorNotFound)?;
                        glfw.create_window(
                            window_args.width as u32,
                            window_args.height as u32,
                            window_args.title,
                            glfw::WindowMode::FullScreen(monitor),
                        )
                        .ok_or(Error::WindowCreationFailed)
                    })?
            }
        };

        // create invisible resource window
        self.glfw.window_hint(glfw::WindowHint::Decorated(false));
        self.glfw.window_hint(glfw::WindowHint::Visible(false));
        let (res_window, res_window_recv) = window
            .create_shared(1, 1, "", glfw::WindowMode::Windowed)
            .ok_or(Error::WindowCreationFailed)?;
        self.glfw.default_window_hints();

        self.window = Some(window);
        self.resource_window = Some(res_window);
        self.resource_window_receiver = Some(res_window_recv);
        let window_ref = if let Some(window) = &mut self.window {
            window as *mut glfw::Window
        } else {
            panic!("The window has vanished");
        };
        let res_window_ref = if let Some(res_window) = &mut self.resource_window {
            res_window as *mut glfw::Window
        } else {
            panic!("The window has vanished");
        };

        // as FlutterEngineRun already calls the make_current callback, user_data must be set now
        self.user_data
            .replace(DesktopUserData::Window(window_ref, res_window_ref));

        // draw initial screen to avoid blinking
        if let Some(window) = self.user_data.borrow_mut().get_window() {
            window.make_current();
            draw::init_gl(window);
            draw::draw_bg(window, window_args.bg_color);
            glfw::make_context_current(None);
        }

        let engine = self.run_flutter_engine(assets_path, icu_data_path, arguments)?;
        // now create the full desktop state
        self.user_data
            .replace(DesktopUserData::WindowState(DesktopWindowState::new(
                window_ref,
                res_window_ref,
                receiver,
                engine,
            )));

        if let DesktopUserData::WindowState(window_state) = &mut *self.user_data.borrow_mut() {
            // send initial size callback to engine
            window_state.send_scale_or_size_change();

            window_state.plugin_registrar.add_system_plugins();

            let window = window_state.window();
            // enable event polling
            window.set_char_polling(true);
            window.set_cursor_pos_polling(true);
            window.set_cursor_enter_polling(true);
            window.set_framebuffer_size_polling(true);
            window.set_key_polling(true);
            window.set_mouse_button_polling(true);
            window.set_scroll_polling(true);
            window.set_size_polling(true);
            window.set_content_scale_polling(true);

            unsafe {
                glfw::ffi::glfwSetWindowRefreshCallback(
                    window.window_ptr(),
                    Some(window_refreshed),
                );
            }
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
                &mut *self.user_data.borrow_mut() as *mut DesktopUserData as *mut std::ffi::c_void,
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

    pub fn init_with_window_state<F>(&mut self, init_fn: F)
    where
        F: FnOnce(&mut DesktopWindowState),
    {
        if let DesktopUserData::WindowState(window_state) = &mut *self.user_data.borrow_mut() {
            init_fn(window_state);
        }
    }

    pub fn run_window_loop(
        mut self,
        mut custom_handler: Option<&mut FnMut(&mut DesktopWindowState, glfw::WindowEvent) -> bool>,
        mut frame_callback: Option<&mut FnMut(&mut DesktopWindowState)>,
    ) {
        if let DesktopUserData::WindowState(window_state) = &mut *self.user_data.borrow_mut() {
            while !window_state.window().should_close() {
                self.glfw.poll_events();
                self.glfw.wait_events_timeout(1.0 / 60.0);

                let events: Vec<(f64, glfw::WindowEvent)> =
                    glfw::flush_messages(&window_state.window_event_receiver).collect();
                for (_, event) in events {
                    let run_default_handler = if let Some(custom_handler) = &mut custom_handler {
                        custom_handler(window_state, event.clone())
                    } else {
                        true
                    };
                    if run_default_handler {
                        window_state.handle_glfw_event(event);
                    }
                }

                window_state.handle_main_thread_callbacks();

                if let Some(callback) = &mut frame_callback {
                    callback(window_state);
                }

                unsafe {
                    flutter_engine_sys::__FlutterEngineFlushPendingTasksNow();
                }
            }
        }
        self.shutdown();
    }

    fn shutdown(self) {
        if let DesktopUserData::WindowState(window_state) = self.user_data.into_inner() {
            window_state.shutdown();
        }
    }
}

fn glfw_error_callback(_error: glfw::Error, description: String, _: &()) {
    error!("GLFW error: {}", description);
}

extern "C" fn window_refreshed(window: *mut glfw::ffi::GLFWwindow) {
    if let Some(engine) = desktop_window_state::get_engine(window) {
        let mut window_size: (i32, i32) = (0, 0);
        let mut framebuffer_size: (i32, i32) = (0, 0);
        let mut scale: (f32, f32) = (0.0, 0.0);

        unsafe {
            glfw::ffi::glfwGetWindowSize(window, &mut window_size.0, &mut window_size.1);
            glfw::ffi::glfwGetFramebufferSize(
                window,
                &mut framebuffer_size.0,
                &mut framebuffer_size.1,
            );
            glfw::ffi::glfwGetWindowContentScale(window, &mut scale.0, &mut scale.1);
        }

        // probably dont need this, since after resize a framebuffer size
        // change event is sent and set this regardless
        // self.window_pixels_per_screen_coordinate =
        //     f64::from(framebuffer_size.0) / f64::from(window_size.0);

        log::debug!(
            "Setting framebuffer size to {:?}, scale to {}",
            framebuffer_size,
            scale.0
        );

        engine.send_window_metrics_event(
            framebuffer_size.0,
            framebuffer_size.1,
            f64::from(scale.0),
        );
    }
}
