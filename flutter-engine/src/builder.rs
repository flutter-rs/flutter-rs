use crate::{FlutterEngine, FlutterEngineHandler};
use std::path::PathBuf;
use std::sync::Weak;

pub struct FlutterEngineBuilder {
    handler: Option<Weak<dyn FlutterEngineHandler>>,
    assets: PathBuf,
    args: Vec<String>,
}

impl FlutterEngineBuilder {
    pub fn new() -> Self {
        Self {
            handler: None,
            assets: Default::default(),
            args: vec![],
        }
    }

    pub fn with_handler(mut self, handler: Weak<dyn FlutterEngineHandler>) -> Self {
        self.handler = Some(handler);
        self
    }

    pub fn with_asset_path(mut self, path: PathBuf) -> Self {
        self.assets = path;
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

    pub fn build(self) -> FlutterEngine {
        let handler = self.handler.expect("No handler set");
        if !handler.upgrade().is_some() {
            panic!("Handler is not valid")
        }

        FlutterEngine::new(handler, self.assets, self.args)
    }
}
