use crate::ffi::{PlatformMessage, PlatformMessageResponseHandle};
use crate::plugins::PluginRegistrar;

use std::sync::mpsc::Receiver;

use log::trace;

const DP_PER_INCH: f64 = 160.0;

pub struct DesktopWindowState {
    pub window: glfw::Window,
    pub window_event_receiver: Receiver<(f64, glfw::WindowEvent)>,
    engine: flutter_engine_sys::FlutterEngine,
    pointer_currently_added: bool,
    monitor_screen_coordinates_per_inch: f64,
    window_pixels_per_screen_coordinate: f64,
    pub plugin_registrar: PluginRegistrar,
}

impl DesktopWindowState {
    pub fn new(
        mut window: glfw::Window,
        window_event_receiver: Receiver<(f64, glfw::WindowEvent)>,
    ) -> Self {
        let monitor_screen_coordinates_per_inch =
            Self::get_screen_coordinates_per_inch(&mut window.glfw);
        Self {
            window,
            window_event_receiver,
            // this has to be set to NULL because the first callbacks that need this state will already be invoked
            // before the engine pointer is returned from FlutterEngineRun
            engine: std::ptr::null_mut(),
            pointer_currently_added: false,
            monitor_screen_coordinates_per_inch,
            window_pixels_per_screen_coordinate: 0.0,
            plugin_registrar: PluginRegistrar::new(),
        }
    }

    fn check_engine(&self) {
        if self.engine.is_null() {
            panic!("Engine was not set!");
        }
    }

    pub fn set_engine(&mut self, engine: flutter_engine_sys::FlutterEngine) {
        if !self.engine.is_null() {
            panic!("Engine was already set!");
        }
        self.engine = engine;
    }

    pub fn get_engine(&self) -> flutter_engine_sys::FlutterEngine {
        self.check_engine();
        self.engine
    }

    pub fn send_framebuffer_size_change(&mut self, framebuffer_size: (i32, i32)) {
        self.check_engine();
        let window_size = self.window.get_size();
        self.window_pixels_per_screen_coordinate = framebuffer_size.0 as f64 / window_size.0 as f64;
        let dpi =
            self.window_pixels_per_screen_coordinate * self.monitor_screen_coordinates_per_inch;
        let pixel_ratio = (dpi / DP_PER_INCH).max(1.0);

        let event = flutter_engine_sys::FlutterWindowMetricsEvent {
            struct_size: std::mem::size_of::<flutter_engine_sys::FlutterWindowMetricsEvent>(),
            width: framebuffer_size.0 as usize,
            height: framebuffer_size.1 as usize,
            pixel_ratio,
        };
        unsafe {
            flutter_engine_sys::FlutterEngineSendWindowMetricsEvent(self.engine, &event);
        }
    }

    pub fn send_platform_message(&self, message: &PlatformMessage) {
        self.check_engine();
        trace!("Sending message on channel {}", message.channel);
        unsafe {
            flutter_engine_sys::FlutterEngineSendPlatformMessage(self.engine, &message.into());
        }
    }

    pub fn send_platform_message_response(
        &self,
        response_handle: PlatformMessageResponseHandle,
        bytes: &[u8],
    ) {
        self.check_engine();
        trace!("Sending message response");
        unsafe {
            flutter_engine_sys::FlutterEngineSendPlatformMessageResponse(
                self.engine,
                response_handle.into(),
                bytes.as_ptr(),
                bytes.len(),
            );
        }
    }

    fn get_screen_coordinates_per_inch(glfw: &mut glfw::Glfw) -> f64 {
        glfw.with_primary_monitor(|glfw, monitor| match monitor {
            None => DP_PER_INCH,
            Some(monitor) => match monitor.get_video_mode() {
                None => DP_PER_INCH,
                Some(video_mode) => {
                    let (width, _) = monitor.get_physical_size();
                    video_mode.width as f64 / (width as f64 / 25.4)
                }
            },
        })
    }
}
