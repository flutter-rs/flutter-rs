use std::{
    env,
    path::PathBuf,
};
use log::{info};
use env_logger;
use flutter::{FlutterProjectArgs, FlutterEngine};

fn main() {
    env_logger::init();
    flutter::init();

    let cwd = match env::var("CARGO_MANIFEST_DIR") {
        Ok(project_dir) => {
            info!("Running inside cargo project");
            PathBuf::from(&project_dir)
        },
        Err(_) => env::current_exe().expect("Cannot get application dir"),
    };

    let assets_path = cwd.join("examples/stocks/flutter_assets");
    let icu_data_path = cwd.join("assets/icudtl.dat");
    let args = FlutterProjectArgs {
        assets_path: assets_path.to_str().unwrap(),
        icu_data_path: icu_data_path.to_str().unwrap(),
    };

    let mut engine = FlutterEngine::new(args);
    engine.run();
    // TODO segfault
    // for some reason this segfault, it does not if put inside run
    // engine.shutdown();
}
