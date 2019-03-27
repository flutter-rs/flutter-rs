extern crate glfw;
extern crate libc;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
extern crate serde;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate log;
extern crate futures;
extern crate tokio;
extern crate tinyfiledialogs;
extern crate gl;

pub mod ffi;
pub mod plugins;
pub mod codec;
pub mod channel;
mod draw;
mod utils;

use std::{
    slice,
    mem,
    borrow::Cow,
    collections::HashMap,
    ptr::{null},
    ffi::{CString, CStr},
    sync::{Arc, Weak, Mutex},
    time::{SystemTime, UNIX_EPOCH},
    cell::{Cell, RefCell},
    sync::mpsc:: { self, Sender, Receiver },
};
use libc::{c_void};
use self::ffi::{
    FlutterOpenGLRendererConfig,
    FlutterRendererType,
    FlutterResult,
    FlutterPlatformMessage,
    FlutterWindowMetricsEvent,
    FlutterEngineRun,
    FlutterEngineSendWindowMetricsEvent,
};
pub use self::plugins::{
    PlatformMessage,
    PluginRegistry,
    Plugin,
    textinput::TextInputPlugin,
    window::WindowPlugin,
    platform::PlatformPlugin,
    dialog::DialogPlugin,
};
use utils::{ CStringVec };
use glfw::{ Context, Action, Key, Modifiers };
use tokio::runtime::Runtime;

pub use glfw::Window;

pub struct FlutterEngineArgs {
    pub assets_path: String,
    pub icu_data_path: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub bg_color: (u8, u8, u8),
    pub window_mode: WindowMode,
    pub command_line_args: Option<Vec<String>>
}

impl Default for FlutterEngineArgs {
    fn default() -> Self {
        FlutterEngineArgs {
            assets_path: String::from(""),
            icu_data_path: String::from(""),
            title: String::from(""),
            width: 1024,
            height: 768,
            bg_color: (255, 255, 255),
            window_mode: WindowMode::Windowed,
            command_line_args: None,
        }
    }
}

const DEFAULT_DPI: f64 = 160.0;

pub enum WindowMode {
    FullScreen(usize), // monitor index
    Windowed,
    Frameless,
}

extern fn present(data: *const c_void) -> bool {
    trace!("present");
    unsafe {
        let window: &mut glfw::Window = &mut *(data as *mut glfw::Window);
        window.swap_buffers();

        // A work around for black screen on window start in macOS Mojave (10.14)
        if cfg!(target_os = "macos") {
            static mut IS_INITIALLY_VISIBLE: bool = false;
            if !IS_INITIALLY_VISIBLE {
                let pos = window.get_pos();
                window.set_pos(pos.0 + 1, pos.1);
                window.set_pos(pos.0, pos.1);
                IS_INITIALLY_VISIBLE = true;
            }
        }
    }
    true
}

extern fn make_current(data: *const c_void) -> bool {
    trace!("make_current");
    unsafe {
        let window: &mut glfw::Window = &mut *(data as *mut glfw::Window);
        window.make_current();
    }
    true
}

extern fn clear_current(_data: *const c_void) -> bool {
    trace!("clear_current");
    glfw::make_context_current(None);
    true
}

extern fn fbo_callback(_data: *const c_void) -> u32 {
    trace!("fbo_callback");
    0
}

extern fn make_resource_current(_data: *const c_void) -> bool {
    trace!("make_resource_current");
    false
}

extern fn gl_proc_resolver(_data: *const c_void, proc: *const libc::c_char) -> *const c_void {
    unsafe {
        return glfw::ffi::glfwGetProcAddress(proc);
    }
}

extern fn platform_message_callback(ptr: *const FlutterPlatformMessage, data: *const c_void) {
    trace!("platform_message_callback");
    unsafe {
        let msg = &*ptr;
        let mmsg = into_platform_message(msg);
        let window: &mut glfw::Window = &mut *(data as *mut glfw::Window);
        if let Some(engine) = FlutterEngine::get_engine(window.window_ptr()) {
            FlutterEngineInner::handle_platform_msg(mmsg, engine, window);
        }
    }
}

extern fn root_isolate_create_callback(_data: *const c_void) {
    trace!("root_isolate_create_callback");
}

extern fn window_refreshed(ptr: *mut glfw::ffi::GLFWwindow) {
    if let Some(engine) = FlutterEngine::get_engine(ptr) {
        let mut w: i32 = 0;
        let mut h: i32 = 0;
        let mut w2: i32 = 0;
        let mut h2: i32 = 0;

        unsafe {
            glfw::ffi::glfwGetWindowSize(ptr, &mut w, &mut h);
            glfw::ffi::glfwGetFramebufferSize(ptr, &mut w2, &mut h2);
        }

        engine.send_window_metrics_change((w, h), (w2, h2));
    }
}

/// consider refactor this with TryFrom trait？
fn into_platform_message(msg: &FlutterPlatformMessage) -> PlatformMessage {
    unsafe {
        let channel = CStr::from_ptr(msg.channel);
        trace!("Unpacking platform msg from channel {:?}", channel);

        let message = slice::from_raw_parts(msg.message, msg.message_size);
        let response_handle = if msg.response_handle == null() {
            None
        } else {
            Some(&*msg.response_handle)
        };
        PlatformMessage {
            channel: Cow::Owned(channel.to_string_lossy().into_owned()),
            message,
            response_handle,
        }
    }
}

fn handle_event(window: &mut glfw::Window, event: glfw::WindowEvent) {
    match event {
        glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
            window.set_should_close(true)
        }
        glfw::WindowEvent::Key(key, _, Action::Press, modifiers) | glfw::WindowEvent::Key(key, _, Action::Repeat, modifiers) => {
            match key {
                Key::Enter => {
                    FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                        if modifiers.contains(Modifiers::Control) {
                            p.perform_action("done");
                        } else {
                            // TODO
                            // why add_char plus newline action?
                            p.add_chars("\n");
                            p.perform_action("newline");
                        }
                    });
                },
                Key::Backspace => {
                    FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                        p.backspace();
                    });
                },
                Key::Delete => {
                    FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                        p.delete();
                    });
                },
                Key::Up => {
                    FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                        p.move_cursor_up(modifiers);
                    });
                },
                Key::Down => {
                    FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                        p.move_cursor_down(modifiers);
                    });
                },
                Key::Left => {
                    FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                        p.move_cursor_left(modifiers);
                    });
                },
                Key::Right => {
                    FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                        p.move_cursor_right(modifiers);
                    });
                },
                Key::Home => {
                    FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                        p.move_cursor_home(modifiers);
                    });
                },
                Key::End => {
                    FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                        p.move_cursor_end(modifiers);
                    });
                },
                Key::A => {
                    if cfg!(target_os = "macos") && modifiers.contains(Modifiers::Super) || modifiers.contains(Modifiers::Control) {
                        FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                            p.select_all();
                        });
                    }
                },
                Key::X => {
                    if cfg!(target_os = "macos") && modifiers.contains(Modifiers::Super) || modifiers.contains(Modifiers::Control) {
                        FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                            let s = p.get_selected_text();
                            p.remove_selected_text();
                            window.set_clipboard_string(&s);
                        });
                    }
                },
                Key::C => {
                    if cfg!(target_os = "macos") && modifiers.contains(Modifiers::Super) || modifiers.contains(Modifiers::Control) {
                        FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                            let s = p.get_selected_text();
                            window.set_clipboard_string(&s);
                        });
                    }
                },
                Key::V => {
                    if cfg!(target_os = "macos") && modifiers.contains(Modifiers::Super) || modifiers.contains(Modifiers::Control) {
                        FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                            let s = window.get_clipboard_string();
                            p.add_chars(&s);
                        });
                    }
                },
                _ => (),
            }
        }
        glfw::WindowEvent::Char(c) => {
            FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                p.add_chars(&c.to_string());
            });
        }
        glfw::WindowEvent::FramebufferSize(w, h) => {
            if let Some(engine) = FlutterEngine::get_engine(window.window_ptr()) {
                let w_size = window.get_size();
                engine.send_window_metrics_change(w_size, (w, h));
            }
        },
        glfw::WindowEvent::CursorPos(x, y) => {
            FlutterEngine::with_plugin(window.window_ptr(), "flutter-rs/window", |p: &Box<WindowPlugin>| {
                // window dragging is handled by window plugin
                if !p.drag_window(window, x, y) {
                    if let Some(engine) = FlutterEngine::get_engine(window.window_ptr()) {
                        if window.get_mouse_button(glfw::MouseButton::Button1) == glfw::Action::Press {
                            let w_size = window.get_size();
                            let size = window.get_framebuffer_size();
                            let pixels_per_screen_coordinate = size.0 as f64 / w_size.0 as f64;
                            engine.send_cursor_position_at_phase(x * pixels_per_screen_coordinate, y * pixels_per_screen_coordinate, ffi::FlutterPointerPhase::Move);
                        }
                    }
                }
            });

        },
        glfw::WindowEvent::MouseButton(button, action, _modifiers) => {
            if button == glfw::MouseButton::Button1 {
                let pos = window.get_cursor_pos();
                let phase = if action == glfw::Action::Press {
                    ffi::FlutterPointerPhase::Down
                } else {
                    ffi::FlutterPointerPhase::Up
                };
                let w_size = window.get_size();
                let size = window.get_framebuffer_size();
                let pixels_per_screen_coordinate = size.0 as f64 / w_size.0 as f64;

                if let Some(engine) = FlutterEngine::get_engine(window.window_ptr()) {
                    engine.send_cursor_position_at_phase(pos.0 * pixels_per_screen_coordinate, pos.1 * pixels_per_screen_coordinate, phase);
                }
            }
        },
        _ => {}
    }
}

pub struct FlutterEngineInner {
    args: FlutterEngineArgs,
    config: ffi::FlutterRendererConfig,
    proj_args: ffi::FlutterProjectArgs,
    ptr: *const ffi::FlutterEngine,
    registry: RefCell<PluginRegistry>,
    glfw: RefCell<Option<(
        glfw::Window,
        std::sync::mpsc::Receiver<(f64, glfw::WindowEvent)>
    )>>,
    dpi: Cell<f64>,
    rt: RefCell<Runtime>, // A tokio async runtime
    tx: Sender<Box<dyn Fn() + Send>>,
    rx: Receiver<Box<dyn Fn() + Send>>,
}

impl FlutterEngineInner {
    fn run(&self) {
        let mut g = glfw::Glfw;

        // setup window
        let mut monitor_id = 0;
        let (mut window, events) = {
            let tip = "Failed to create GLFW window.";
            match self.args.window_mode {
                WindowMode::Frameless => {
                    g.window_hint(glfw::WindowHint::Decorated(false));
                    g.create_window(
                        self.args.width,
                        self.args.height,
                        &self.args.title,
                        glfw::WindowMode::Windowed,
                    ).expect(tip)
                },
                WindowMode::FullScreen(idx) => {
                    monitor_id = idx;
                    g.with_connected_monitors(|g, monitors| {
                        let monitor = monitors.get(idx).expect("Cannot find specified monitor");
                        g.create_window(
                            self.args.width,
                            self.args.height,
                            &self.args.title,
                            glfw::WindowMode::FullScreen(&monitor),
                        ).expect(tip)
                    })
                },
                _ => {
                    g.create_window(
                        self.args.width,
                        self.args.height,
                        &self.args.title,
                        glfw::WindowMode::Windowed,
                    ).expect(tip)
                },
            }
        };

        self.dpi.set(self.get_dpi(&mut g, monitor_id));

        window.set_key_polling(true);
        window.set_framebuffer_size_polling(true);
        window.set_size_polling(true);
        window.set_mouse_button_polling(true);
        window.set_cursor_pos_polling(true);
        window.set_char_polling(true);
        window.make_current();

        self.add_system_plugins();

        // poll_events is blocked during window resize. This callback fix redraw freeze during window resize.
        // See https://github.com/glfw/glfw/issues/408 for details
        unsafe {
            glfw::ffi::glfwSetWindowRefreshCallback(
                window.window_ptr(),
                Some(window_refreshed)
            );
        }

        // move window and events to FlutterEngineInner struct
        self.glfw.replace(Some((window, events)));

        self.with_window_mut(|window| {
            // draw inital background color
            draw::init_gl(window);
            draw::draw_bg(window, &self.args);

            unsafe {
                let ret = FlutterEngineRun(
                    1,
                    &self.config,
                    &self.proj_args,
                    window as *const glfw::Window as *const c_void,
                    &self.ptr as *const *const ffi::FlutterEngine);

                assert!(ret == FlutterResult::Success, "Cannot start flutter engine");
            }

            let window_size = window.get_size();
            let buf_size = window.get_framebuffer_size();
            self.send_window_metrics_change(window_size, buf_size);
        });
    }

    fn with_window_mut(&self, cbk: impl FnOnce(&mut glfw::Window)) {
        let mut pack = self.glfw.borrow_mut();
        let (window, _) = pack.as_mut().unwrap();
        cbk(window);
    }

    pub fn with_async(&self, cbk: impl FnOnce(&mut Runtime)) {
        let rt = &mut *self.rt.borrow_mut();
        cbk(rt);
    }

    pub fn ui_thread(&self, f: Box<dyn Fn() + Send>) {
        let _ = self.tx.send(f);
    }

    fn add_system_plugins(&self) {
        let registry = &mut *self.registry.borrow_mut();

        let plugin = TextInputPlugin::new();
        registry.add_plugin(Box::new(plugin));

        let plugin = PlatformPlugin::new();
        registry.add_plugin(Box::new(plugin));

        let plugin = DialogPlugin::new();
        registry.add_plugin(Box::new(plugin));

        let plugin = WindowPlugin::new();
        registry.add_plugin(Box::new(plugin));
    }

    fn event_loop(&self) {
        if let Some((window, events)) = &mut *self.glfw.borrow_mut() {
            while !window.should_close() {
                // glfw.poll_events();
                // window.glfw.wait_events();
                window.glfw.wait_events_timeout(1.0/60.0);

                for (_, event) in glfw::flush_messages(&events) {
                    handle_event(window, event);
                }

                // This is required, otherwise windows won't trigger platform_message_callback
                unsafe {
                    ffi::__FlutterEngineFlushPendingTasksNow();
                }

                // process ui thread callback queue
                for v in self.rx.try_recv() {
                   v();
                }
            }
        }
    }

    fn get_dpi(&self, glfw: &mut glfw::Glfw, monitor_id: usize) -> f64 {
        glfw.with_connected_monitors(|_, monitors| {
            let m = monitors.get(monitor_id).unwrap();
            let mode = m.get_video_mode().unwrap();
            let physical_size = m.get_physical_size();
            if physical_size.0 <= 0 {
                return 160.0;
            }
            mode.width as f64 / (physical_size.0 as f64 / 25.4)
        })
    }

    fn send_window_metrics_change(&self, window_size: (i32, i32), buf_size: (i32, i32)) {
        let pixel_ratio = buf_size.0 as f64 / window_size.0 as f64 * self.dpi.get() / DEFAULT_DPI;
        let evt = FlutterWindowMetricsEvent {
            struct_size: mem::size_of::<FlutterWindowMetricsEvent>(),
            width: buf_size.0 as usize,
            height: buf_size.1 as usize,
            pixel_ratio,
        };
        unsafe {
            FlutterEngineSendWindowMetricsEvent(self.ptr, &evt as *const FlutterWindowMetricsEvent);
        }
    }

    fn send_cursor_position_at_phase(&self, x: f64, y: f64, phase: ffi::FlutterPointerPhase) {
        let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let evt = &ffi::FlutterPointerEvent {
            struct_size: mem::size_of::<ffi::FlutterPointerEvent>(),
            timestamp: (duration.as_secs() as f64 * 1e6 + duration.subsec_nanos() as f64 / 1e3) as usize,
            phase: phase,
            x: x,
            y: y,
        };

        unsafe {
            ffi::FlutterEngineSendPointerEvent(
                self.ptr,
                evt,
                1
            );
        }
    }

    pub fn send_platform_message(&self, message: &PlatformMessage) {
        trace!("Sending message {:?} on channel {}", message, message.channel);
        let msg: FlutterPlatformMessage = message.into();
        unsafe {
            ffi::FlutterEngineSendPlatformMessage(
                self.ptr,
                &msg as *const ffi::FlutterPlatformMessage,
            );
        }
        // we need to manually drop this message
        // msg.drop();
    }

    pub fn send_platform_message_response(&self, response_handle: &ffi::FlutterPlatformMessageResponseHandle, bytes: &[u8]) {
        trace!("Sending message response");
        unsafe {
            ffi::FlutterEngineSendPlatformMessageResponse(
                self.ptr,
                response_handle,
                bytes as *const [u8] as *const _,
                bytes.len(),
            );
        }
    }

    fn handle_platform_msg(msg: PlatformMessage, engine: Arc<FlutterEngineInner>, window: &mut glfw::Window) {
        let mut registry = engine.registry.borrow_mut();
        registry.handle(msg, engine.clone(), window);
    }
}

// Has to be Send and Sync to use with lazy_static
unsafe impl Send for FlutterEngineInner {}
unsafe impl Sync for FlutterEngineInner {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash,)]
struct WindowKey(*mut glfw::ffi::GLFWwindow);

unsafe impl Send for WindowKey {}
unsafe impl Sync for WindowKey {}

lazy_static! {
    static ref ENGINES: Mutex<HashMap<WindowKey, Weak<FlutterEngineInner>>> = Mutex::new(HashMap::new());
}

// Use Arc, since ENGINES need to have a weak ref of FlutterEngineInner
pub struct FlutterEngine {
    inner: Arc<FlutterEngineInner>,
}

impl FlutterEngine {
    pub fn new(args: FlutterEngineArgs) -> FlutterEngine {
        let config: ffi::FlutterRendererConfig = ffi::FlutterRendererConfig {
            kind: FlutterRendererType::OpenGL,
            open_gl: FlutterOpenGLRendererConfig {
                struct_size: mem::size_of::<FlutterOpenGLRendererConfig>(),
                make_current: make_current,
                clear_current: clear_current,
                present: present,
                fbo_callback: fbo_callback,
                make_resource_current: make_resource_current,
                fbo_reset_after_present: false,
                surface_transformation: None,
                gl_proc_resolver: gl_proc_resolver,
                gl_external_texture_frame_callback: None,
            },
        };

        // FlutterProjectArgs is expecting a full argv, so when processing it for flags the first
        // item is treated as the executable and ignored. Add a dummy value so that all provided arguments
        // are used.
        let vm_args = {
            let mut cli_args = vec!["placeholder"];
            if let Some(a) = &args.command_line_args {
                cli_args.extend(a.iter().map(|v| v.as_str()));
            } else {
                // use default args
                cli_args.push("--observatory-port=50300");
            };
            CStringVec::new(&cli_args)
        };

        let proj_args = ffi::FlutterProjectArgs {
            struct_size: mem::size_of::<ffi::FlutterProjectArgs>(),
            assets_path: CString::new(args.assets_path.to_string()).unwrap().into_raw(),
            main_path: CString::new("").unwrap().into_raw(),
            packages_path: CString::new("").unwrap().into_raw(),
            icu_data_path: CString::new(args.icu_data_path.to_string()).unwrap().into_raw(),
            command_line_argc: vm_args.len() as i32,
            command_line_argv: vm_args.into_raw(),
            platform_message_callback: platform_message_callback,
            vm_snapshot_data: std::ptr::null(),
            vm_snapshot_data_size: 0,
            vm_snapshot_instructions: std::ptr::null(),
            vm_snapshot_instructions_size: 0,
            isolate_snapshot_data: std::ptr::null(),
            isolate_snapshot_data_size: 0,
            isolate_snapshot_instructions: std::ptr::null(),
            isolate_snapshot_instructions_size: 0,
            root_isolate_create_callback: root_isolate_create_callback,
        };

        info!("Project args {:?}", proj_args);
        info!("OpenGL config {:?}", config);

        let (tx, rx) = mpsc::channel();
        let inner = Arc::new(FlutterEngineInner {
            args,
            config,
            proj_args,
            ptr: null(),
            registry: RefCell::new(PluginRegistry::new()),
            glfw: RefCell::new(None),
            dpi: Cell::new(DEFAULT_DPI),
            rt: RefCell::new(Runtime::new().expect("Cannot init tokio runtime")),
            tx,
            rx,
        });
        inner.registry.borrow_mut().set_engine(Arc::downgrade(&inner));
        FlutterEngine {
            inner,
        }
    }

    pub fn run(&self) {
        self.inner.run();
        {
            let glfw = &*self.inner.glfw.borrow();
            let (window, _) = glfw.as_ref().unwrap();
            let mut guard = ENGINES.lock().unwrap();
            guard.insert(WindowKey(window.window_ptr()), Arc::downgrade(&self.inner));
        }
        self.inner.event_loop();
    }

    pub fn shutdown(&self) {
        unsafe {
            ffi::FlutterEngineShutdown(self.inner.ptr);
        }
    }

    pub fn add_plugin(&self, plugin: Box<dyn Plugin>) {
        let mut registry = self.inner.registry.borrow_mut();
        registry.add_plugin(plugin);
    }

    fn get_engine(window_ptr: *mut glfw::ffi::GLFWwindow) -> Option<Arc<FlutterEngineInner>> {
        let guard = ENGINES.lock().unwrap();
        if let Some(weak) = guard.get(&WindowKey(window_ptr)) {
            weak.upgrade()
        } else {
            None
        }
    }

    fn with_plugin<T, F: FnMut(&Box<T>)>(window_ptr: *mut glfw::ffi::GLFWwindow, channel: &str, mut cbk: F) {
        if let Some(engine) = FlutterEngine::get_engine(window_ptr) {
            if let Some(plugin) = engine.registry.borrow().get_plugin(channel) {
                unsafe {
                    let p = std::mem::transmute::<&Box<dyn Plugin>, &Box<T>>(plugin);
                    cbk(p);
                }
            }
        }
    }
}

impl Drop for FlutterEngine {
    fn drop(&mut self) {
        // TODO: destroy window?
    }
}

/// init must be called from the main thread
pub fn init() {
    glfw::init(glfw::FAIL_ON_ERRORS).unwrap();
}
