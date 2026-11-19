//! Builder for [`FlutterEngine`].

use crate::tasks::TaskRunnerHandler;
use crate::view::{FlutterCompositorHandler, ViewFocusChangeHandler};
use crate::{CreateError, FlutterEngine, FlutterOpenGLHandler};
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::Arc;

/// Collects everything an engine needs before it can be initialised.
///
/// A platform handler and an OpenGL handler are mandatory; everything else is
/// optional.
/// Which renderer backend the engine should start with.
///
/// OpenGL is the default on Linux and Windows. Targets whose engine has no
/// OpenGL backend (macOS) pick [`RendererKind::Metal`] or
/// [`RendererKind::Software`] instead.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum RendererKind {
    #[default]
    OpenGl,
    /// Rasterise on the CPU and hand the pixels to the compositor. Works
    /// everywhere, but the compositor has to upload every frame.
    Software,
    /// Render into embedder-provided Metal textures. The handles are opaque
    /// `id<MTLDevice>` / `id<MTLCommandQueue>` pointers owned by the caller,
    /// which must outlive the engine.
    Metal { device: usize, command_queue: usize },
}

/// Collects everything an engine needs before it can be initialised.
///
/// A platform handler and an OpenGL handler are mandatory; everything else is
/// optional.
#[derive(Default)]
pub struct FlutterEngineBuilder {
    pub(crate) platform_handler: Option<Arc<dyn TaskRunnerHandler + Send + Sync>>,
    pub(crate) opengl_handler: Option<Box<dyn FlutterOpenGLHandler + Send + Sync>>,
    pub(crate) compositor_handler: Option<Box<dyn FlutterCompositorHandler + Send + Sync>>,
    pub(crate) view_focus_change_handler: Option<ViewFocusChangeHandler>,
    pub(crate) ui_on_platform_thread: bool,
    pub(crate) renderer: RendererKind,
    pub(crate) engine_id: Option<i64>,
    pub(crate) assets: PathBuf,
    pub(crate) icu_data: Option<PathBuf>,
    pub(crate) args: Vec<String>,
}

impl core::fmt::Debug for FlutterEngineBuilder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FlutterEngineBuilder")
            .field("has_platform_handler", &self.platform_handler.is_some())
            .field("has_opengl_handler", &self.opengl_handler.is_some())
            .field("has_compositor_handler", &self.compositor_handler.is_some())
            .field(
                "has_view_focus_change_handler",
                &self.view_focus_change_handler.is_some(),
            )
            .field("ui_on_platform_thread", &self.ui_on_platform_thread)
            .field("renderer", &self.renderer)
            .field("engine_id", &self.engine_id)
            .field("assets", &self.assets)
            .field("icu_data", &self.icu_data)
            .field("args", &self.args)
            .finish()
    }
}

impl FlutterEngineBuilder {
    /// An empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The handler woken whenever the engine posts a platform-thread task.
    ///
    /// The thread that calls [`build`](Self::build) becomes the engine's
    /// platform thread.
    #[must_use]
    pub fn with_platform_handler(
        mut self,
        handler: Arc<dyn TaskRunnerHandler + Send + Sync>,
    ) -> Self {
        self.platform_handler = Some(handler);
        self
    }

    /// The OpenGL context the engine renders through.
    #[must_use]
    pub fn with_opengl<H>(mut self, handler: H) -> Self
    where
        H: FlutterOpenGLHandler + Send + Sync + 'static,
    {
        self.opengl_handler = Some(Box::new(handler));
        self
    }

    /// Install a per-view compositor.
    ///
    /// Supplying a compositor switches the engine away from the legacy
    /// single-view `fbo_callback`/`present` rendering path: the engine allocates
    /// a backing store per view through [`FlutterCompositorHandler`] and
    /// presents each view through
    /// [`FlutterCompositorHandler::present_view`]. This is required for
    /// multiple views.
    #[must_use]
    pub fn with_compositor<H>(mut self, handler: H) -> Self
    where
        H: FlutterCompositorHandler + Send + Sync + 'static,
    {
        self.compositor_handler = Some(Box::new(handler));
        self
    }

    /// Run the Dart UI isolate on the platform thread.
    ///
    /// This mirrors the stock desktop embedders and is required for
    /// synchronous view creation from Dart: `FlutterEngineAddView` posts to the
    /// UI task runner and must run before the embedder returns the view id to
    /// Dart. With the UI runner merged into the platform runner, that task runs
    /// inline.
    #[must_use]
    pub fn with_ui_on_platform_thread(mut self) -> Self {
        self.ui_on_platform_thread = true;
        self
    }

    /// Render with the engine's software renderer.
    ///
    /// The engine then rasterises into CPU memory, which the compositor
    /// receives as a software backing store, instead of rendering through
    /// OpenGL. The macOS embedder ships without an OpenGL backend, so targets
    /// there must use this or [`with_metal_renderer`](Self::with_metal_renderer).
    /// Supplying a compositor is still required for multi-view presentation.
    #[must_use]
    pub fn with_software_renderer(mut self) -> Self {
        self.renderer = RendererKind::Software;
        self
    }

    /// Render into embedder-provided Metal textures.
    ///
    /// `device` and `command_queue` must be valid `id<MTLDevice>` and
    /// `id<MTLCommandQueue>` handles that outlive the engine and stay valid for
    /// every callback. The engine renders each view's backing store into a
    /// texture the compositor creates, and submits its work on this queue; a
    /// compositor that presents on the same queue gets naturally ordered
    /// commands.
    #[must_use]
    pub fn with_metal_renderer(
        mut self,
        device: *const c_void,
        command_queue: *const c_void,
    ) -> Self {
        self.renderer = RendererKind::Metal {
            device: device as usize,
            command_queue: command_queue as usize,
        };
        self
    }

    /// The opaque value the engine exposes to Dart as
    /// `PlatformDispatcher.engineId`.
    ///
    /// The stock embedders put a native engine pointer here. flutter-glfw uses
    /// it for its windowing function table address. When unset, a unique
    /// non-zero counter is used.
    #[must_use]
    pub fn with_engine_id(mut self, engine_id: i64) -> Self {
        self.engine_id = Some(engine_id);
        self
    }

    /// Handle the framework's request to move native focus to a view.
    ///
    /// The framework calls this from `FocusScope` changes; the embedder should
    /// focus the native window for `view_id` (and later report the resulting
    /// native focus change back with
    /// [`FlutterEngine::send_view_focus_event`](crate::FlutterEngine::send_view_focus_event)).
    #[must_use]
    pub fn with_view_focus_change_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(i64, bool) + Send + Sync + 'static,
    {
        self.view_focus_change_handler = Some(Box::new(handler));
        self
    }

    /// Path to the application's `flutter_assets` directory.
    #[must_use]
    pub fn with_asset_path(mut self, path: PathBuf) -> Self {
        self.assets = path;
        self
    }

    /// Path to `icudtl.dat`.
    ///
    /// Official Flutter engine builds no longer embed the ICU data inside
    /// `libflutter_engine`, so it must be supplied separately. When set, the
    /// engine is pointed at this file via `icu_data_path`; when omitted the
    /// engine falls back to the legacy linked-in `gIcudtl` symbol prefix.
    #[must_use]
    pub fn with_icu_data_path(mut self, path: PathBuf) -> Self {
        self.icu_data = Some(path);
        self
    }

    /// Add one engine switch, e.g. `--disable-dart-asserts`.
    #[must_use]
    pub fn with_arg(mut self, arg: String) -> Self {
        self.args.push(arg);
        self
    }

    /// Add several engine switches.
    #[must_use]
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args.extend(args);
        self
    }

    /// Initialise the engine. It is not running yet — call
    /// [`FlutterEngine::run`] next, on this same thread.
    ///
    /// # Errors
    ///
    /// Returns [`CreateError`] when a mandatory handler is missing, when an
    /// argument or path contains an interior NUL byte, or when the engine
    /// refuses the configuration.
    pub fn build(self) -> Result<FlutterEngine, CreateError> {
        FlutterEngine::new(self)
    }
}
