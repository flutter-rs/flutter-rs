use crate::{ffi::FlutterEngine, plugins::PluginRegistrar};

use std::{cell::RefCell, rc::Rc, sync::mpsc::Receiver};

const DP_PER_INCH: f64 = 160.0;

pub struct DesktopWindowState {
    pub runtime_data: Rc<RuntimeData>,
    pointer_currently_added: bool,
    monitor_screen_coordinates_per_inch: f64,
    window_pixels_per_screen_coordinate: f64,
    pub plugin_registrar: PluginRegistrar,
}

pub struct RuntimeData {
    pub window: Rc<RefCell<glfw::Window>>,
    pub window_event_receiver: Receiver<(f64, glfw::WindowEvent)>,
    pub engine: Rc<FlutterEngine>,
}

impl DesktopWindowState {
    pub fn new(
        window: Rc<RefCell<glfw::Window>>,
        window_event_receiver: Receiver<(f64, glfw::WindowEvent)>,
        engine: FlutterEngine,
    ) -> Self {
        let monitor_screen_coordinates_per_inch =
            Self::get_screen_coordinates_per_inch(&mut window.borrow_mut().glfw);
        let runtime_data = Rc::new(RuntimeData {
            window,
            window_event_receiver,
            engine: Rc::new(engine),
        });
        Self {
            pointer_currently_added: false,
            monitor_screen_coordinates_per_inch,
            window_pixels_per_screen_coordinate: 0.0,
            plugin_registrar: PluginRegistrar::new(Rc::downgrade(&runtime_data)),
            runtime_data,
        }
    }

    pub fn send_framebuffer_size_change(&mut self, framebuffer_size: (i32, i32)) {
        let window_size = self.runtime_data.window.borrow().get_size();
        self.window_pixels_per_screen_coordinate = framebuffer_size.0 as f64 / window_size.0 as f64;
        let dpi =
            self.window_pixels_per_screen_coordinate * self.monitor_screen_coordinates_per_inch;
        let pixel_ratio = (dpi / DP_PER_INCH).max(1.0);
        self.runtime_data.engine.send_window_metrics_event(
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
