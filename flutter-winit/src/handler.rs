use crate::context::Context;
use crate::window::FlutterEvent;
use copypasta::{ClipboardContext, ClipboardProvider};
use flutter_engine::tasks::TaskRunnerHandler;
use flutter_engine::FlutterOpenGLHandler;
use flutter_plugins::platform::{AppSwitcherDescription, MimeError, PlatformHandler};
use flutter_plugins::textinput::TextInputHandler;
use flutter_plugins::window::{PositionParams, WindowHandler};
use glutin::event_loop::EventLoopProxy;
use parking_lot::Mutex;
use std::error::Error;
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// TODO: Investigate removing mutex
pub struct WinitPlatformTaskHandler {
    proxy: Mutex<EventLoopProxy<FlutterEvent>>,
}

impl WinitPlatformTaskHandler {
    pub fn new(proxy: EventLoopProxy<FlutterEvent>) -> Self {
        Self {
            proxy: Mutex::new(proxy),
        }
    }
}

impl TaskRunnerHandler for WinitPlatformTaskHandler {
    fn wake(&self) {
        self.proxy
            .lock()
            .send_event(FlutterEvent::WakePlatformThread)
            .ok();
    }
}

pub struct WinitOpenGLHandler {
    context: Arc<Mutex<Context>>,
    resource_context: Arc<Mutex<Context>>,
}

impl WinitOpenGLHandler {
    pub fn new(context: Arc<Mutex<Context>>, resource_context: Arc<Mutex<Context>>) -> Self {
        Self {
            context,
            resource_context,
        }
    }
}

impl FlutterOpenGLHandler for WinitOpenGLHandler {
    fn swap_buffers(&self) -> bool {
        self.context.lock().present()
    }

    fn make_current(&self) -> bool {
        unsafe { self.context.lock().make_current() }
    }

    fn clear_current(&self) -> bool {
        unsafe { self.context.lock().make_not_current() }
    }

    fn fbo_callback(&self) -> u32 {
        0
    }

    fn make_resource_current(&self) -> bool {
        unsafe { self.resource_context.lock().make_current() }
    }

    fn gl_proc_resolver(&self, proc: *const c_char) -> *mut c_void {
        unsafe {
            if let Ok(proc) = CStr::from_ptr(proc).to_str() {
                return self.context.lock().get_proc_address(proc) as _;
            }
            std::ptr::null_mut()
        }
    }
}

pub struct WinitPlatformHandler {
    clipboard: ClipboardContext,
    context: Arc<Mutex<Context>>,
}

impl WinitPlatformHandler {
    pub fn new(context: Arc<Mutex<Context>>) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            clipboard: ClipboardContext::new()?,
            context,
        })
    }
}

impl PlatformHandler for WinitPlatformHandler {
    fn set_application_switcher_description(&mut self, description: AppSwitcherDescription) {
        self.context.lock().window().set_title(&description.label);
    }

    fn set_clipboard_data(&mut self, text: String) {
        if let Err(err) = self.clipboard.set_contents(text) {
            log::error!("{}", err);
        }
    }

    fn get_clipboard_data(&mut self, mime: &str) -> Result<String, MimeError> {
        if mime != "text/plain" {
            return Err(MimeError);
        }
        let result = self.clipboard.get_contents();
        if let Err(err) = &result {
            log::error!("{}", err);
        }
        Ok(result.unwrap_or_default())
    }
}

pub struct WinitWindowHandler {
    context: Arc<Mutex<Context>>,
    maximized: bool,
    visible: bool,
    close: Arc<AtomicBool>,
}

impl WinitWindowHandler {
    pub fn new(context: Arc<Mutex<Context>>, close: Arc<AtomicBool>) -> Self {
        Self {
            context,
            maximized: false,
            visible: false,
            close,
        }
    }
}

impl WindowHandler for WinitWindowHandler {
    fn close(&mut self) {
        self.close.store(true, Ordering::Relaxed);
    }

    fn show(&mut self) {
        self.visible = true;
        self.context.lock().window().set_visible(self.visible);
    }

    fn hide(&mut self) {
        self.visible = false;
        self.context.lock().window().set_visible(self.visible);
    }

    fn is_visible(&mut self) -> bool {
        self.visible
    }

    fn maximize(&mut self) {
        self.maximized = true;
        self.context.lock().window().set_maximized(self.maximized);
    }

    fn restore(&mut self) {
        self.maximized = false;
        self.context.lock().window().set_maximized(self.maximized);
    }

    fn is_maximized(&mut self) -> bool {
        self.maximized
    }

    fn iconify(&mut self) {}

    fn is_iconified(&mut self) -> bool {
        false
    }

    fn set_pos(&mut self, _pos: PositionParams) {}

    fn get_pos(&mut self) -> PositionParams {
        PositionParams { x: 0.0, y: 0.0 }
    }

    fn start_drag(&mut self) {}

    fn end_drag(&mut self) {}
}

pub struct WinitTextInputHandler {}

impl Default for WinitTextInputHandler {
    fn default() -> Self {
        Self {}
    }
}

impl TextInputHandler for WinitTextInputHandler {
    fn show(&mut self) {}

    fn hide(&mut self) {}
}
