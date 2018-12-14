use std::{
    env,
    path::PathBuf,
};
use log::{info};
use env_logger;
use flutter::{FlutterProjectArgs, FlutterEngine};

#[cfg(target_os = "macos")]
use core_foundation::bundle;


fn main() {
    env_logger::init();
    flutter::init();

    let (assets_path, icu_data_path) = match env::var("CARGO_MANIFEST_DIR") {
        Ok(project_dir) => {
            info!("Running inside cargo project");
            let cwd = PathBuf::from(&project_dir);
            (
                cwd.join("examples/stocks/flutter_assets"),
                cwd.join("assets/icudtl.dat"),
            )
        },
        Err(_) => {
            let res = if cfg!(target_os = "macos") {
                let bd = bundle::CFBundle::main_bundle();
                let exe = bd.executable_url().expect("Cannot get executable dir").to_path().expect("to_path error");
                exe.parent().unwrap().parent().unwrap().join("Resources")
            } else {
                env::current_exe().expect("Cannot get application dir")
                    .parent().expect("Cannot get application dir")
                    .to_path_buf()
            };
            (
                res.join("flutter_assets"),
                res.join("icudtl.dat"),
            )
        },
    };

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
