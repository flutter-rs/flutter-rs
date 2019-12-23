use glutin::dpi::{LogicalSize, PhysicalSize};
use glutin::window::Window;
use glutin::{ContextWrapper, NotCurrent, PossiblyCurrent};
use std::ffi::c_void;

pub struct Context(Option<ContextWrapper<PossiblyCurrent, Window>>);

impl Context {
    pub fn from_context(ctx: ContextWrapper<NotCurrent, Window>) -> Self {
        Self(Some(unsafe { ctx.treat_as_current() }))
    }

    pub fn empty() -> Self {
        Self(None)
    }

    pub fn context(&self) -> Option<&ContextWrapper<PossiblyCurrent, Window>> {
        self.0.as_ref()
    }

    pub unsafe fn make_current(&mut self) -> bool {
        if let Some(ctx) = self.0.take() {
            if let Ok(ctx) = ctx.make_current() {
                self.0 = Some(ctx);
                return true;
            }
        }
        false
    }

    pub unsafe fn make_not_current(&mut self) -> bool {
        if let Some(ctx) = self.0.take() {
            if let Ok(ctx) = ctx.make_not_current() {
                self.0 = Some(ctx.treat_as_current());
                return true;
            }
        }
        false
    }

    pub fn get_proc_address(&self, proc: &str) -> *const c_void {
        if let Some(ctx) = self.0.as_ref() {
            return ctx.get_proc_address(proc);
        }
        std::ptr::null()
    }

    pub fn resize(&self, size: PhysicalSize) {
        if let Some(ctx) = self.0.as_ref() {
            ctx.resize(size)
        }
    }

    pub fn present(&self) -> bool {
        if let Some(ctx) = self.0.as_ref() {
            return ctx.swap_buffers().is_ok();
        }
        false
    }

    pub fn size(&self) -> LogicalSize {
        self.0.as_ref().unwrap().window().inner_size()
    }

    pub fn hidpi_factor(&self) -> f64 {
        self.0.as_ref().unwrap().window().hidpi_factor()
    }
}
