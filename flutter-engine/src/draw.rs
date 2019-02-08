use glfw::Context;
use crate::FlutterEngineArgs;
use std::u8;
// include the OpenGL type aliases
// use gl::types::*;

pub fn init_gl(window: &mut glfw::Window) {
    gl::load_with(|s| window.get_proc_address(s));
}

/// Draw blank background before flutter engine starts.
pub fn draw_bg(window: &mut glfw::Window, args: &FlutterEngineArgs) {
    unsafe {
        let r = args.bg_color.0 as f32 / u8::MAX as f32;
        let g = args.bg_color.1 as f32 / u8::MAX as f32;
        let b = args.bg_color.2 as f32 / u8::MAX as f32;
        gl::ClearColor(r, g, b, 1.0);
        gl::Clear(gl::COLOR_BUFFER_BIT);
        window.swap_buffers();
    }
}