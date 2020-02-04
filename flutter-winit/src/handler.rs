use crate::window::FlutterEvent;
use async_std::task;
use copypasta::{ClipboardContext, ClipboardProvider};
use crossbeam::atomic::AtomicCell;
use flutter_engine::FlutterEngineHandler;
use flutter_plugins::platform::{AppSwitcherDescription, MimeError, PlatformHandler};
use flutter_plugins::window::{PositionParams, WindowHandler};
use futures_task::FutureObj;
use glutin::context::Context;
use glutin::surface::{Surface, Window as WindowMarker};
use parking_lot::Mutex;
use std::error::Error;
use std::ffi::CStr;
use std::future::Future;
use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Current {
    Main,
    Resource,
    None,
}

pub struct WinitFlutterEngineHandler {
    proxy: EventLoopProxy<FlutterEvent>,
    surface: Arc<Mutex<Option<Surface<WindowMarker>>>>,
    context: Arc<Context>,
    resource_context: Option<Arc<Context>>,
    current: AtomicCell<Current>,
}

impl WinitFlutterEngineHandler {
    pub fn new(
        proxy: EventLoopProxy<FlutterEvent>,
        surface: Arc<Mutex<Option<Surface<WindowMarker>>>>,
        context: Arc<Context>,
        resource_context: Option<Arc<Context>>,
    ) -> Self {
        Self {
            proxy,
            surface,
            context,
            resource_context,
            current: AtomicCell::new(Current::None),
        }
    }
}

impl FlutterEngineHandler for WinitFlutterEngineHandler {
    fn swap_buffers(&self) -> bool {
        if let Some(surf) = self.surface.lock().as_ref() {
            surf.swap_buffers().is_ok()
        } else {
            log::error!("swap_buffers: no surface");
            false
        }
    }

    fn make_current(&self) -> bool {
        let res = if let Some(surf) = self.surface.lock().as_ref() {
            unsafe { self.context.make_current(surf) }
        } else {
            log::error!("make_current: no surface");
            return false;
        };
        match res {
            Ok(()) => {
                self.current.store(Current::Main);
                true
            }
            Err(err) => {
                log::error!("{}", err);
                false
            }
        }
    }

    fn clear_current(&self) -> bool {
        match unsafe { self.context.make_not_current() } {
            Ok(()) => {
                self.current.store(Current::None);
                true
            }
            Err(err) => {
                log::error!("{}", err);
                false
            }
        }
    }

    fn fbo_callback(&self) -> u32 {
        0
    }

    fn make_resource_current(&self) -> bool {
        if let Some(context) = &self.resource_context {
            match unsafe { context.make_current_surfaceless() } {
                Ok(()) => {
                    self.current.store(Current::Resource);
                    true
                }
                Err(err) => {
                    log::error!("{}", err);
                    false
                }
            }
        } else {
            false
        }
    }

    fn gl_proc_resolver(&self, proc: *const c_char) -> *mut c_void {
        unsafe {
            if let Ok(proc) = CStr::from_ptr(proc).to_str() {
                let context = match self.current.load() {
                    Current::Main => &self.context,
                    Current::Resource => self.resource_context.as_ref().unwrap(),
                    Current::None => {
                        log::error!("no context is current");
                        return std::ptr::null_mut();
                    }
                };
                if let Ok(ptr) = context.get_proc_address(proc) {
                    return ptr as _;
                }
            }
        }
        std::ptr::null_mut()
    }

    fn wake_platform_thread(&self) {
        self.proxy.send_event(FlutterEvent::WakePlatformThread).ok();
    }

    fn run_in_background(&self, future: Box<dyn Future<Output = ()> + Send + 'static>) {
        task::spawn(FutureObj::new(future));
    }
}

pub struct WinitPlatformHandler {
    clipboard: ClipboardContext,
    window: Arc<Window>,
}

impl WinitPlatformHandler {
    pub fn new(window: Arc<Window>) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            clipboard: ClipboardContext::new()?,
            window,
        })
    }
}

impl PlatformHandler for WinitPlatformHandler {
    fn set_application_switcher_description(&mut self, description: AppSwitcherDescription) {
        self.window.set_title(&description.label);
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
    window: Arc<Window>,
    maximized: bool,
    visible: bool,
    close: Arc<AtomicBool>,
}

impl WinitWindowHandler {
    pub fn new(window: Arc<Window>, close: Arc<AtomicBool>) -> Self {
        Self {
            window,
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
        self.window.set_visible(self.visible);
    }

    fn hide(&mut self) {
        self.visible = false;
        self.window.set_visible(self.visible);
    }

    fn is_visible(&mut self) -> bool {
        self.visible
    }

    fn maximize(&mut self) {
        self.maximized = true;
        self.window.set_maximized(self.maximized);
    }

    fn restore(&mut self) {
        self.maximized = false;
        self.window.set_maximized(self.maximized);
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
