use crate::context::Context;
use crate::window::FlutterEvent;
use async_std::task;
use copypasta::{ClipboardContext, ClipboardProvider};
use flutter_engine::ffi::{ExternalTextureFrame, TextureId};
use flutter_engine::texture_registry::TextureRegistry;
use flutter_engine::{FlutterEngine, FlutterEngineHandler};
use flutter_plugins::platform::{AppSwitcherDescription, MimeError, PlatformHandler};
use flutter_plugins::window::{PositionParams, WindowHandler};
use futures_task::FutureObj;
use glutin::event_loop::EventLoopProxy;
use image::RgbaImage;
use parking_lot::Mutex;
use std::error::Error;
use std::ffi::CStr;
use std::future::Future;
use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct WinitFlutterEngineHandler {
    proxy: EventLoopProxy<FlutterEvent>,
    context: Arc<Mutex<Context>>,
    resource_context: Arc<Mutex<Context>>,
    texture_registry: Arc<TextureRegistry>,
}

impl WinitFlutterEngineHandler {
    pub fn new(
        proxy: EventLoopProxy<FlutterEvent>,
        context: Arc<Mutex<Context>>,
        resource_context: Arc<Mutex<Context>>,
        texture_registry: Arc<TextureRegistry>,
    ) -> Self {
        Self {
            proxy,
            context,
            resource_context,
            texture_registry,
        }
    }
}

impl FlutterEngineHandler for WinitFlutterEngineHandler {
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

    fn wake_platform_thread(&self) {
        self.proxy.send_event(FlutterEvent::WakePlatformThread).ok();
    }

    fn run_in_background(&self, future: Box<dyn Future<Output = ()> + Send + 'static>) {
        task::spawn(FutureObj::new(future));
    }

    fn create_texture(&self, engine: &FlutterEngine, img: RgbaImage) -> TextureId {
        self.texture_registry.create_texture(engine, img)
    }

    fn get_texture_frame(
        &self,
        texture_id: i64,
        size: (usize, usize),
    ) -> Option<ExternalTextureFrame> {
        self.texture_registry.get_texture_frame(texture_id, size)
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
