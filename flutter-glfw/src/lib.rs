use crate::window::{WindowArgs, FlutterWindow, CreateError};
use log::error;

//mod desktop_window_state;
mod draw;
//mod utils;
mod handler;
pub mod window;

pub fn init() -> Result<FlutterDesktop, glfw::InitError> {
    glfw::init(Some(glfw::Callback {
        f: glfw_error_callback,
        data: (),
    }))
        .map(|glfw| FlutterDesktop {
            glfw,
        })
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn glfw_error_callback(error: glfw::Error, description: String, _: &()) {
    error!("GLFW error ({}): {}", error, description);
}

pub struct FlutterDesktop {
    glfw: glfw::Glfw,
}

impl FlutterDesktop {
    pub fn create_window(&mut self, window_args: &WindowArgs) -> Result<FlutterWindow, CreateError> {
        FlutterWindow::create(&mut self.glfw, window_args)
    }
}
