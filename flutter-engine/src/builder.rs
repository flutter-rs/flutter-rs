//! Builder for [`FlutterEngine`].

use crate::tasks::TaskRunnerHandler;
use crate::{CreateError, FlutterEngine, FlutterOpenGLHandler};
use std::path::PathBuf;
use std::sync::Arc;

/// Collects everything an engine needs before it can be initialised.
///
/// A platform handler and an OpenGL handler are mandatory; everything else is
/// optional.
#[derive(Default)]
pub struct FlutterEngineBuilder {
    pub(crate) platform_handler: Option<Arc<dyn TaskRunnerHandler + Send + Sync>>,
    pub(crate) opengl_handler: Option<Box<dyn FlutterOpenGLHandler + Send + Sync>>,
    pub(crate) assets: PathBuf,
    pub(crate) icu_data: Option<PathBuf>,
    pub(crate) args: Vec<String>,
}

impl core::fmt::Debug for FlutterEngineBuilder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FlutterEngineBuilder")
            .field("has_platform_handler", &self.platform_handler.is_some())
            .field("has_opengl_handler", &self.opengl_handler.is_some())
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
