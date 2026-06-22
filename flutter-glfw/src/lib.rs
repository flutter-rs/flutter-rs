use crate::window::{CreateError, FlutterWindow, WindowArgs};
use log::error;
use std::path::PathBuf;

mod handler;
mod keymap;
pub mod window;

pub fn init() -> Result<FlutterDesktop, glfw::InitError> {
    glfw::init(glfw_error_callback).map(|glfw| FlutterDesktop { glfw })
}

fn glfw_error_callback(error: glfw::Error, description: String) {
    error!("GLFW error ({}): {}", error, description);
}

pub struct FlutterDesktop {
    glfw: glfw::Glfw,
}

impl FlutterDesktop {
    pub fn create_window(
        &mut self,
        window_args: &WindowArgs,
        assets_path: PathBuf,
        arguments: Vec<String>,
    ) -> Result<FlutterWindow, CreateError> {
        FlutterWindow::create(&mut self.glfw, window_args, assets_path, None, arguments)
    }

    /// Like [`create_window`](Self::create_window) but also points the engine
    /// at an external `icudtl.dat`, required by official engine builds that no
    /// longer link the ICU data into `libflutter_engine`.
    pub fn create_window_with_icu(
        &mut self,
        window_args: &WindowArgs,
        assets_path: PathBuf,
        icu_data_path: PathBuf,
        arguments: Vec<String>,
    ) -> Result<FlutterWindow, CreateError> {
        FlutterWindow::create(
            &mut self.glfw,
            window_args,
            assets_path,
            Some(icu_data_path),
            arguments,
        )
    }

    pub fn glfw(&self) -> glfw::Glfw {
        self.glfw.clone()
    }
}
