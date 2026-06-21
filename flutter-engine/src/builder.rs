use crate::tasks::TaskRunnerHandler;
use crate::{CreateError, FlutterEngine, FlutterOpenGLHandler};
use std::path::PathBuf;
use std::sync::Arc;

pub struct FlutterEngineBuilder {
    pub(crate) platform_handler: Option<Arc<dyn TaskRunnerHandler + Send + Sync>>,
    pub(crate) opengl_handler: Option<Box<dyn FlutterOpenGLHandler + Send>>,
    pub(crate) assets: PathBuf,
    pub(crate) icu_data: Option<PathBuf>,
    pub(crate) args: Vec<String>,
}

impl FlutterEngineBuilder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            platform_handler: None,
            opengl_handler: None,
            assets: Default::default(),
            icu_data: None,
            args: vec![],
        }
    }

    pub fn with_platform_handler(
        mut self,
        handler: Arc<dyn TaskRunnerHandler + Send + Sync>,
    ) -> Self {
        self.platform_handler = Some(handler);
        self
    }

    pub fn with_opengl<H>(mut self, handler: H) -> Self
    where
        H: FlutterOpenGLHandler + Send + 'static,
    {
        self.opengl_handler = Some(Box::new(handler));
        self
    }

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
    pub fn with_icu_data_path(mut self, path: PathBuf) -> Self {
        self.icu_data = Some(path);
        self
    }

    pub fn with_arg(mut self, arg: String) -> Self {
        self.args.push(arg);
        self
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        for arg in args.into_iter() {
            self.args.push(arg);
        }
        self
    }

    pub fn build(self) -> Result<FlutterEngine, CreateError> {
        FlutterEngine::new(self)
    }
}
