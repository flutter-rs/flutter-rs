use crate::ffi::{FlutterEngine, PlatformMessage, PlatformMessageResponseHandle};
use crate::plugins::PluginRegistrar;

use std::sync::mpsc::Receiver;

use log::trace;

const DP_PER_INCH: f64 = 160.0;

pub struct DesktopWindowState {
    pub window: glfw::Window,
    pub window_event_receiver: Receiver<(f64, glfw::WindowEvent)>,
    engine: Option<FlutterEngine>,
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
            // this has to be set to None because the first callbacks that need this state will already be invoked
            // before the engine pointer is returned from FlutterEngineRun
            engine: None,
            pointer_currently_added: false,
            monitor_screen_coordinates_per_inch,
            window_pixels_per_screen_coordinate: 0.0,
            plugin_registrar: PluginRegistrar::new(),
        }
    }

    fn check_engine(&self) {
        if self.engine.is_none() {
            panic!("Engine was not set!");
        }
    }

    pub fn set_engine(&mut self, engine: flutter_engine_sys::FlutterEngine) {
        if self.engine.is_some() {
            panic!("Engine was already set!");
        }
        self.engine = FlutterEngine::new(engine);
    }

    pub fn get_engine(&self) -> FlutterEngine {
        self.check_engine();
        self.engine.unwrap()
    }

    pub fn send_framebuffer_size_change(&mut self, framebuffer_size: (i32, i32)) {
        self.check_engine();
        let window_size = self.window.get_size();
        self.window_pixels_per_screen_coordinate = framebuffer_size.0 as f64 / window_size.0 as f64;
        let dpi =
            self.window_pixels_per_screen_coordinate * self.monitor_screen_coordinates_per_inch;
        let pixel_ratio = (dpi / DP_PER_INCH).max(1.0);
        self.engine.unwrap().send_window_metrics_event(
            framebuffer_size.0,
            framebuffer_size.1,
            pixel_ratio,
        );
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
