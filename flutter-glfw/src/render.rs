//! Render-thread state shared by the GL handler and the compositor.
//!
//! The engine renders every view into a backing store while the *master*
//! context is current, and the compositor presents each view into its own
//! window context. Both halves need the same map, so it lives here behind
//! `Arc<Mutex<..>>` rather than on the main-thread window state.

use glfw::PRenderContext;
use parking_lot::{Condvar, Mutex};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The master context, the shared resource context, and every visible view's
/// presentation context.
pub(crate) struct RenderState {
    /// The context the engine renders into backing stores with.
    pub master: Arc<Mutex<PRenderContext>>,
    /// Per-view presentation contexts and cached geometry.
    pub views: Mutex<HashMap<i64, ViewRender>>,
    /// Signalled whenever a view's framebuffer size changes.
    size_settled: Condvar,
    /// Wakes the platform thread's event loop after `set_desired_size`.
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

/// What the compositor needs to know about one visible view.
pub(crate) struct ViewRender {
    /// The GLFW context of the view's window.
    pub context: Arc<Mutex<PRenderContext>>,
    /// Last reported framebuffer size in physical pixels.
    pub framebuffer_size: (i32, i32),
    /// Framebuffer size confirmed by a `FramebufferSize` event.
    ///
    /// A size set through GLFW can be read back before the window server has
    /// applied it, so presentation waits on this value instead of
    /// [`Self::framebuffer_size`].
    pub settled_size: (i32, i32),
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
            size_settled: Condvar::new(),
            wake: Mutex::new(None),
        })
    }

    /// Installs the callback that wakes the platform thread's event loop.
    pub(crate) fn set_wake(&self, wake: impl Fn() + Send + Sync + 'static) {
        *self.wake.lock() = Some(Arc::new(wake));
    }

    /// Wakes the platform thread so it applies a pending desired size.
    pub(crate) fn wake(&self) {
        if let Some(wake) = self.wake.lock().as_ref() {
            wake();
        }
    }

    /// Waits until `view_id`'s framebuffer reports `size` physical pixels.
    ///
    /// Returns `false` if the view disappeared or the resize did not settle
    /// within `timeout`.
    pub(crate) fn wait_for_size(&self, view_id: i64, size: (i32, i32), timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut views = self.views.lock();
        loop {
            match views.get(&view_id) {
                None => return false,
                Some(view)
                    if (view.settled_size.0 - size.0).abs() <= 1
                        && (view.settled_size.1 - size.1).abs() <= 1 =>
                {
                    return true;
                }
                Some(_) => {}
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let _ = self.size_settled.wait_for(&mut views, deadline - now);
        }
    }

    /// Records that a `FramebufferSize` event confirmed the view's size.
    pub(crate) fn mark_size_settled(&self, view_id: i64) {
        if let Some(view) = self.views.lock().get_mut(&view_id) {
            view.settled_size = view.framebuffer_size;
        }
        self.size_settled.notify_all();
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
                settled_size: framebuffer_size,
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
        self.size_settled.notify_all();
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
