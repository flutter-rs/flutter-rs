extern crate glfw;
extern crate libc;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
extern crate serde;
#[macro_use]
extern crate serde_json;
#[macro_use] extern crate log;
extern crate env_logger;

pub mod ffi;
pub mod plugins;
mod utils;

use std::{
    slice,
    mem,
    collections::HashMap,
    ptr::{null},
    ffi::{CString, CStr},
    sync::{Arc, Weak, Mutex},
    time::{SystemTime, UNIX_EPOCH},
    cell::RefCell,
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
use self::plugins::{
    PlatformMessage,
    Message,
    PluginRegistry,
    Plugin,
    textinput::TextInputPlugin,
    platform::PlatformPlugin,
};
use utils::{CStringVec};
use glfw::{Context, Action, Key, Modifiers};

pub struct FlutterProjectArgs<'a> {
    pub assets_path: &'a str,
    pub icu_data_path: &'a str,
}

extern fn present(data: *const c_void) -> bool {
    trace!("present");
    unsafe {
        let window: &mut glfw::Window = &mut *(data as *mut glfw::Window);
        window.swap_buffers();
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

extern fn platform_message_callback(ptr: *const FlutterPlatformMessage, data: *const c_void) {
    match into_platform_message(ptr) {
        Ok(msg) => {
            info!("Got msg {:?}", msg);
            unsafe {
                let window: &mut glfw::Window = &mut *(data as *mut glfw::Window);
                handle_platform_message(window, msg);
            }
        },
        Err(err) => {
            error!("Decode msg error {:?}", err);
        }
    }
}

/// consider refactor this with TryFrom trait？
fn into_platform_message(ptr: *const FlutterPlatformMessage) -> Result<PlatformMessage, serde_json::Error> {
    unsafe {
        let msg = &*ptr;
        let channel = CStr::from_ptr(msg.channel);
        let s = std::str::from_utf8_unchecked(slice::from_raw_parts(msg.message, msg.message_size));
        serde_json::from_str::<Message>(&s).map(|message| {
            PlatformMessage {
                channel: channel.to_string_lossy().into_owned(),
                message: message,
                response_handle: if msg.response_handle == null() { None } else { Some(10) } // TODO fix this handle
            }
        })        
    }
}

fn handle_platform_message(window: &mut glfw::Window, msg: PlatformMessage) {
    if let Some(engine) = FlutterEngine::get_engine(window.window_ptr()) {
        engine.handle_platform_msg(msg, window);
    }
}

fn handle_window_event(window: &mut glfw::Window, event: glfw::WindowEvent) {
    match event {
        glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
            window.set_should_close(true)
        }
        glfw::WindowEvent::Key(key, _, Action::Press, modifiers) => {
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
            if let Some(engine) = FlutterEngine::get_engine(window.window_ptr()) {
                if window.get_mouse_button(glfw::MouseButton::Button1) == glfw::Action::Press {
                    let w_size = window.get_size();
                    let size = window.get_framebuffer_size();
                    let pixel_ratio = size.0 as f64 / w_size.0 as f64;
                    engine.send_cursor_position_at_phase(x * pixel_ratio, y * pixel_ratio, ffi::FlutterPointerPhase::Move);
                }
            }
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
                let pixel_ratio = size.0 as f64 / w_size.0 as f64;

                if let Some(engine) = FlutterEngine::get_engine(window.window_ptr()) {
                    engine.send_cursor_position_at_phase(pos.0 * pixel_ratio, pos.1 * pixel_ratio, phase);
                }
            }
        },
        _ => {}
    }
}

pub struct FlutterEngineInner {
    config: ffi::FlutterRendererConfig,
    args: ffi::FlutterProjectArgs,
    ptr: *const ffi::FlutterEngine,
    plugins: RefCell<PluginRegistry>,
}

impl FlutterEngineInner {
    fn send_window_metrics_change(&self, w_size: (i32, i32), size: (i32, i32)) {
        let evt = FlutterWindowMetricsEvent {
            struct_size: mem::size_of::<FlutterWindowMetricsEvent>(),
            width: size.0 as usize,
            height: size.1 as usize,
            pixel_ratio: size.0 as f64/ w_size.0 as f64,
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

    fn send_platform_message(&self, message: PlatformMessage) {
        trace!("Sending message {:?}", message);
        let mut msg: FlutterPlatformMessage = message.into();
        unsafe {
            ffi::FlutterEngineSendPlatformMessage(
                self.ptr,
                &msg as *const ffi::FlutterPlatformMessage,
            );
        }
        // we need to manually drop this message
        msg.drop();
    }

    fn send_platform_message_response(&self) {

    }

    fn handle_platform_msg(&self, msg: PlatformMessage, window: &mut glfw::Window) {
        self.plugins.borrow_mut().handle(msg, self, window);
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
pub struct FlutterEngine{
    engine: Arc<FlutterEngineInner>,
}

impl FlutterEngine {
    pub fn new(_args: FlutterProjectArgs) -> FlutterEngine {
        let config: ffi::FlutterRendererConfig = ffi::FlutterRendererConfig {
            kind: FlutterRendererType::OpenGL,
            open_gl: FlutterOpenGLRendererConfig {
                struct_size: mem::size_of::<FlutterOpenGLRendererConfig>(),
                make_current: make_current,
                clear_current: clear_current,
                present: present,
                fbo_callback: fbo_callback,
                make_resource_current: make_resource_current,
            },
        };

        let main_path = CString::new("").unwrap();
        let packages_path = CString::new("").unwrap();
        let vm_args = CStringVec::new(&["--dart-non-checked-mode", "--observatory-port=50300"]);
        let args = ffi::FlutterProjectArgs {
            struct_size: mem::size_of::<ffi::FlutterProjectArgs>(),
            assets_path: CString::new(_args.assets_path).unwrap().into_raw(),
            main_path: main_path.into_raw(),
            packages_path: packages_path.into_raw(),
            icu_data_path: CString::new(_args.icu_data_path).unwrap().into_raw(),
            command_line_argc: vm_args.len() as i32,
            command_line_argv: vm_args.into_raw(),
            platform_message_callback: platform_message_callback,
        };

        info!("Project args {:?}", args);
        info!("OpenGL config {:?}", config);

        let inner = Arc::new(FlutterEngineInner {
            config: config,
            args: args,
            ptr: null(),
            plugins: RefCell::new(PluginRegistry::new()),
        });
        inner.plugins.borrow_mut().set_engine(Arc::downgrade(&inner));
        FlutterEngine {
            engine: inner,
        }
    }

    pub fn run(&mut self) {
//     -> (glfw::Glfw, glfw::Window, std::sync::mpsc::Receiver<(f64, glfw::WindowEvent)>) {
        let mut glfw = glfw::Glfw;

        let (mut window, events) = glfw.create_window(400, 550, "Flutter Demo", glfw::WindowMode::Windowed)
            .expect("Failed to create GLFW window.");

        self.add_system_plugins();
        window.set_key_polling(true);
        window.set_framebuffer_size_polling(true);
        window.set_size_polling(true);
        window.set_mouse_button_polling(true);
        window.set_cursor_pos_polling(true);
        window.set_char_polling(true);
        window.make_current();

        unsafe {
            let w = &mut window as *mut glfw::Window;
            let ret = FlutterEngineRun(1, &self.engine.config, &self.engine.args, w as *const c_void, &self.engine.ptr as *const *const ffi::FlutterEngine);
            assert!(ret == FlutterResult::Success);

            let w_size = window.get_size();
            let size = window.get_framebuffer_size();
            self.engine.send_window_metrics_change(w_size, size);
        }

        {
            let mut guard = ENGINES.lock().unwrap();
            guard.insert(WindowKey(window.window_ptr()), Arc::downgrade(&self.engine));
        }

        while !window.should_close() {
            glfw.poll_events();
            // glfw.wait_events();
            // engine.flush_pending_tasks_now();
            for (_, event) in glfw::flush_messages(&events) {
                handle_window_event(&mut window, event);
            }
        }
    }

    fn add_system_plugins(&self) {
        let plugins = &mut *self.engine.plugins.borrow_mut();

        plugins.add_plugin(Box::new(TextInputPlugin::new(&self.engine)));

        let platform_plugin: PlatformPlugin = Default::default();
        plugins.add_plugin(Box::new(platform_plugin));
    }

    pub fn shutdown(&mut self) {
        unsafe {
            ffi::FlutterEngineShutdown(self.engine.ptr);
        }
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
            if let Some(plugin) = engine.plugins.borrow().get_plugin(channel) {
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