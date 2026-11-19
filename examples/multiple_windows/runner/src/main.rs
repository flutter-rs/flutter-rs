//! Runs Flutter's `multiple_windows` example on the flutter-glfw backend.
//!
//! The Rust side starts one engine and the hidden render contexts; every
//! visible window is created from Dart by the `flutter_glfw_windowing`
//! `WindowingOwner`.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let assets = std::env::var_os("FLUTTER_ASSET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("build/flutter_assets"));
    let icu = std::env::var_os("FLUTTER_ICU_DATA").map(PathBuf::from);

    let mut desktop = flutter_glfw::init()?;
    let mut app_args = flutter_glfw::AppArgs::new(assets);
    app_args.icu_data_path = icu;
    // Debugging aid for renderer experiments, e.g. `--enable-impeller` or
    // `--verbose-logging`.
    if let Ok(args) = std::env::var("FLUTTER_ENGINE_ARGS") {
        app_args.engine_args = args.split_whitespace().map(str::to_owned).collect();
    }

    let app = desktop.create_app(app_args)?;
    app.run(None, None)?;
    app.shutdown();
    Ok(())
}
