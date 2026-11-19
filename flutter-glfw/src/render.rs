//! Render-thread state shared by the GL handler and the compositor.
//!
//! The engine renders every view into a backing store while the *master*
//! context is current, and the compositor presents each view into its own
//! window context. Both halves need the same map, so it lives here behind
//! `Arc<Mutex<..>>` rather than on the main-thread window state.

use glfw::PRenderContext;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

/// The master context, the shared resource context, and every visible view's
/// presentation context.
pub(crate) struct RenderState {
    /// The context the engine renders into backing stores with.
    pub master: Arc<Mutex<PRenderContext>>,
    /// Per-view presentation contexts and cached geometry.
    pub views: Mutex<HashMap<i64, ViewRender>>,
}

/// What the compositor needs to know about one visible view.
pub(crate) struct ViewRender {
    /// The GLFW context of the view's window.
    pub context: Arc<Mutex<PRenderContext>>,
    /// Last reported framebuffer size in physical pixels.
    pub framebuffer_size: (i32, i32),
    /// Physical pixels per logical pixel.
    pub scale: f64,
    /// Whether the framework sizes this view to its content.
    pub shrink_wrap: bool,
    /// A content size requested by the last presented frame; the main thread
    /// applies it with `glfwSetWindowSize`.
    pub desired_size: Mutex<Option<(f64, f64)>>,
}

impl RenderState {
    pub(crate) fn new(master: Arc<Mutex<PRenderContext>>) -> Arc<Self> {
        Arc::new(Self {
            master,
            views: Mutex::new(HashMap::new()),
        })
    }

    pub(crate) fn insert_view(
        &self,
        view_id: i64,
        context: Arc<Mutex<PRenderContext>>,
        framebuffer_size: (i32, i32),
        scale: f64,
        shrink_wrap: bool,
    ) {
        self.views.lock().insert(
            view_id,
            ViewRender {
                context,
                framebuffer_size,
                scale,
                shrink_wrap,
                desired_size: Mutex::new(None),
            },
        );
    }

    pub(crate) fn remove_view(&self, view_id: i64) {
        self.views.lock().remove(&view_id);
    }

    pub(crate) fn update_metrics(&self, view_id: i64, framebuffer_size: (i32, i32), scale: f64) {
        if let Some(view) = self.views.lock().get_mut(&view_id) {
            view.framebuffer_size = framebuffer_size;
            view.scale = scale;
        }
    }

    /// Take the pending shrink-wrap size for `view_id`, if any.
    pub(crate) fn take_desired_size(&self, view_id: i64) -> Option<(f64, f64)> {
        self.views
            .lock()
            .get(&view_id)
            .and_then(|view| view.desired_size.lock().take())
    }

    /// Publish the content size of a shrink-wrapped view for the main thread.
    pub(crate) fn set_desired_size(&self, view_id: i64, size: (f64, f64)) {
        if let Some(view) = self.views.lock().get(&view_id) {
            *view.desired_size.lock() = Some(size);
        }
    }
}
