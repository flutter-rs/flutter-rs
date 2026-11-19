//! Views, window metrics and the per-view compositor hook.
//!
//! A Flutter engine can own many views. The implicit view (id `0`) always
//! exists; additional views are created with [`FlutterEngine::add_view`] and
//! torn down with [`FlutterEngine::remove_view`]. Each view needs its own
//! window metrics and, when the engine is configured with a compositor, its own
//! render target.
//!
//! [`FlutterEngine::add_view`]: crate::FlutterEngine::add_view
//! [`FlutterEngine::remove_view`]: crate::FlutterEngine::remove_view

use flutter_engine_sys::{FlutterBackingStore, FlutterLayer, FlutterWindowMetricsEvent};
use std::mem;

/// The engine's identifier for a view.
///
/// `0` is the implicit view; the embedder cannot add or remove it.
pub type FlutterViewId = i64;

/// The implicit view's id.
pub const IMPLICIT_VIEW_ID: FlutterViewId = 0;

/// Constraints applied to a view's logical size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewConstraints {
    /// Minimum physical width.
    pub min_width: usize,
    /// Maximum physical width.
    pub max_width: usize,
    /// Minimum physical height.
    pub min_height: usize,
    /// Maximum physical height.
    pub max_height: usize,
}

impl ViewConstraints {
    /// Create constraints, returning `None` when they are inconsistent.
    #[must_use]
    pub fn new(
        min_width: usize,
        max_width: usize,
        min_height: usize,
        max_height: usize,
    ) -> Option<Self> {
        if min_width > max_width || min_height > max_height {
            return None;
        }
        Some(Self {
            min_width,
            max_width,
            min_height,
            max_height,
        })
    }
}

/// Metrics for one view.
///
/// Physical sizes are in device pixels and `pixel_ratio` converts them to
/// logical pixels. Regular windows should use [`WindowMetrics::tight`]: the
/// view fills the window. Shrink-wrapped windows use [`WindowMetrics::loose`]
/// with a large maximum so the framework's `RenderView` can size itself to its
/// child (the embedder then resizes the native window to the presented frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowMetrics {
    /// Physical width in device pixels.
    pub width: usize,
    /// Physical height in device pixels.
    pub height: usize,
    /// Physical pixels per logical pixel.
    pub pixel_ratio: f64,
    /// Optional size constraints; `None` means tight metrics.
    pub constraints: Option<ViewConstraints>,
    /// Display identifier; `0` until display plumbing is wired.
    pub display_id: u64,
}

impl WindowMetrics {
    /// Tight metrics: the view fills exactly `width` × `height` pixels.
    #[must_use]
    pub fn tight(width: usize, height: usize, pixel_ratio: f64) -> Self {
        Self {
            width,
            height,
            pixel_ratio,
            constraints: None,
            display_id: 0,
        }
    }

    /// Loose metrics: the view may shrink-wrap its content between `min` and
    /// `max` physical pixels.
    #[must_use]
    pub fn loose(
        width: usize,
        height: usize,
        pixel_ratio: f64,
        max_width: usize,
        max_height: usize,
    ) -> Self {
        Self {
            width,
            height,
            pixel_ratio,
            constraints: ViewConstraints::new(1, max_width.max(1), 1, max_height.max(1)),
            display_id: 0,
        }
    }

    /// Attach explicit constraints to these metrics.
    #[must_use]
    pub fn with_constraints(mut self, constraints: ViewConstraints) -> Self {
        self.constraints = Some(constraints);
        self
    }

    /// Set the display id.
    #[must_use]
    pub fn with_display_id(mut self, display_id: u64) -> Self {
        self.display_id = display_id;
        self
    }

    /// `true` when the engine will accept these metrics.
    ///
    /// A 0×0 or otherwise degenerate size is rejected: the engine returns
    /// `kInvalidArguments` for metrics outside their constraints, and an
    /// iconified GLFW window reports a zero framebuffer. Callers should skip
    /// the update instead of sending it.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        if self.width == 0
            || self.height == 0
            || !self.pixel_ratio.is_finite()
            || self.pixel_ratio <= 0.0
        {
            return false;
        }
        match self.constraints {
            None => true,
            Some(c) => {
                self.width >= c.min_width
                    && self.width <= c.max_width
                    && self.height >= c.min_height
                    && self.height <= c.max_height
            }
        }
    }

    /// Build the C event, clamping the size into the constraints.
    ///
    /// Returns `None` when the metrics are degenerate and should not be sent.
    #[must_use]
    pub fn to_event(&self, view_id: FlutterViewId) -> Option<FlutterWindowMetricsEvent> {
        if self.width == 0
            || self.height == 0
            || !self.pixel_ratio.is_finite()
            || self.pixel_ratio <= 0.0
        {
            return None;
        }
        let (width, height, has_constraints, min_width, max_width, min_height, max_height) =
            match self.constraints {
                None => (self.width, self.height, false, 0, 0, 0, 0),
                Some(c) => (
                    self.width.clamp(c.min_width, c.max_width),
                    self.height.clamp(c.min_height, c.max_height),
                    true,
                    c.min_width,
                    c.max_width,
                    c.min_height,
                    c.max_height,
                ),
            };
        Some(FlutterWindowMetricsEvent {
            struct_size: mem::size_of::<FlutterWindowMetricsEvent>(),
            width,
            height,
            pixel_ratio: self.pixel_ratio,
            left: 0,
            top: 0,
            physical_view_inset_top: 0.0,
            physical_view_inset_right: 0.0,
            physical_view_inset_bottom: 0.0,
            physical_view_inset_left: 0.0,
            display_id: self.display_id,
            view_id,
            has_constraints,
            min_width_constraint: min_width,
            min_height_constraint: min_height,
            max_width_constraint: max_width,
            max_height_constraint: max_height,
        })
    }
}

/// Why a view could not be added or removed.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ViewError {
    /// The call was made from a thread other than the engine's platform thread.
    #[error("view operations must be called on the platform thread")]
    NotPlatformThread,
    /// The engine has been shut down.
    #[error("the engine has been shut down")]
    ShutDown,
    /// The embedder API rejected the request.
    #[error("the engine rejected the view operation with code {0}")]
    Engine(i32),
    /// The engine did not invoke the completion callback in time.
    #[error("timed out waiting for the engine to complete the view operation")]
    Timeout,
    /// The engine reported that the view was not added.
    #[error("the engine did not add the view")]
    NotAdded,
    /// The engine reported that the view was not removed.
    #[error("the engine did not remove the view")]
    NotRemoved,
    /// The implicit view (`0`) cannot be added or removed.
    #[error("the implicit view cannot be added or removed")]
    ImplicitView,
    /// The supplied metrics were degenerate (for example 0×0) or outside their
    /// constraints.
    #[error("the view metrics are invalid")]
    InvalidMetrics,
}

/// The engine's per-view compositor callbacks.
///
/// Implementations are called on the engine's raster thread. Every layer and
/// backing store pointer passed here is valid only for the duration of the
/// callback; do not store them. The handler is shared by every view of one
/// engine, so state keyed by view id must be synchronized.
///
/// Backing stores are created while the embedder's *raster* GL context is
/// current. When presenting into another GL context, only objects shared by the
/// share group (textures, buffers, renderbuffers) are valid; framebuffer object
/// names are per-context and must be recreated there.
pub trait FlutterCompositorHandler: Send + Sync {
    /// Create a backing store for `view_id`, filling `store` on success.
    ///
    /// `size` is in physical pixels and may be 0×0 for the implicit view before
    /// the first window reports metrics; return `false` for sizes you cannot
    /// support.
    fn create_backing_store(
        &self,
        view_id: FlutterViewId,
        size: (usize, usize),
        store: &mut FlutterBackingStore,
    ) -> bool;

    /// Release a backing store previously returned by
    /// [`create_backing_store`](Self::create_backing_store).
    ///
    /// The C callback carries no view id; keep it in the store's `user_data` if
    /// you need it.
    fn collect_backing_store(&self, store: &FlutterBackingStore);

    /// Composite the layers of `view_id` onto its native target.
    fn present_view(&self, view_id: FlutterViewId, layers: &[&FlutterLayer]) -> bool;
}

/// The framework asked the embedder to change the native focus of a view.
pub type ViewFocusChangeHandler = Box<dyn Fn(FlutterViewId, bool) + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tight_metrics_are_valid_and_tight() {
        let metrics = WindowMetrics::tight(800, 600, 1.5);
        assert!(metrics.is_valid());
        let event = metrics.to_event(3).expect("valid metrics");
        assert_eq!(event.view_id, 3);
        assert_eq!(event.width, 800);
        assert_eq!(event.height, 600);
        assert_eq!(event.pixel_ratio, 1.5);
        assert!(!event.has_constraints);
    }

    #[test]
    fn loose_metrics_clamp_into_constraints() {
        let metrics = WindowMetrics::loose(10_000, 10_000, 1.0, 4_000, 3_000);
        assert!(!metrics.is_valid());
        let event = metrics.to_event(1).expect("clamped");
        assert!(event.has_constraints);
        assert_eq!(event.width, 4_000);
        assert_eq!(event.height, 3_000);
        assert_eq!(event.min_width_constraint, 1);
        assert_eq!(event.max_width_constraint, 4_000);
    }

    #[test]
    fn degenerate_metrics_are_rejected() {
        assert!(WindowMetrics::tight(0, 600, 1.0).to_event(0).is_none());
        assert!(WindowMetrics::tight(800, 0, 1.0).to_event(0).is_none());
        assert!(WindowMetrics::tight(800, 600, 0.0).to_event(0).is_none());
        assert!(
            WindowMetrics::tight(800, 600, f64::NAN)
                .to_event(0)
                .is_none()
        );
    }

    #[test]
    fn inconsistent_constraints_are_dropped() {
        assert!(ViewConstraints::new(10, 5, 1, 10).is_none());
        let metrics = WindowMetrics::tight(800, 600, 1.0)
            .with_constraints(ViewConstraints::new(100, 1_000, 100, 800).expect("valid"));
        let event = metrics.to_event(0).expect("valid");
        assert!(event.has_constraints);
        assert_eq!(event.min_width_constraint, 100);
        assert_eq!(event.max_width_constraint, 1_000);
    }
}
