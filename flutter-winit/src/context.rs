use glutin::dpi::{LogicalSize, PhysicalSize};
use glutin::window::Window;
use glutin::{ContextWrapper, NotCurrent};
use std::ffi::c_void;

pub struct Context(Option<ContextWrapper<NotCurrent, Window>>);

impl Context {
    pub fn from_context(ctx: ContextWrapper<NotCurrent, Window>) -> Self {
        Self(Some(ctx))
    }

    pub fn empty() -> Self {
        Self(None)
    }

    pub fn context(&self) -> Option<&ContextWrapper<NotCurrent, Window>> {
        self.0.as_ref()
    }

    pub unsafe fn make_current(&mut self) -> bool {
        if let Some(ctx) = self.0.take() {
            if let Ok(ctx) = ctx.make_current() {
                self.0 = Some(ctx.treat_as_not_current());
                return true;
            }
        }
        false
    }

    pub unsafe fn make_not_current(&mut self) -> bool {
        if let Some(ctx) = self.0.take() {
            if let Ok(ctx) = ctx.make_not_current() {
                self.0 = Some(ctx);
                return true;
            }
        }
        false
    }

    pub fn get_proc_address(&mut self, proc: &str) -> *const c_void {
        if let Some(ctx) = self.0.take() {
            let ctx = unsafe { ctx.treat_as_current() };
            let result = ctx.get_proc_address(proc);
            let ctx = unsafe { ctx.treat_as_not_current() };
            self.0 = Some(ctx);
            return result;
        }
        std::ptr::null()
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if let Some(ctx) = self.0.take() {
            let ctx = unsafe { ctx.treat_as_current() };
            ctx.resize(size);
            let ctx = unsafe { ctx.treat_as_not_current() };
            self.0 = Some(ctx);
        }
    }

    pub fn present(&mut self) -> bool {
        if let Some(ctx) = self.0.take() {
            let ctx = unsafe { ctx.treat_as_current() };
            let result = ctx.swap_buffers().is_ok();
            let ctx = unsafe { ctx.treat_as_not_current() };
            self.0 = Some(ctx);
            return result;
        }
        false
    }

    pub fn window(&self) -> &Window {
        self.0.as_ref().unwrap().window()
    }

    pub fn size(&self) -> LogicalSize<u32> {
        self.window().inner_size().to_logical(self.hidpi_factor())
    }

    pub fn hidpi_factor(&self) -> f64 {
        self.window().scale_factor()
    }
}
