** flutter-engine is a library to make desktop apps in flutter and rust **

## flutter-engine in action

```rust
use std::{
    env,
    path::PathBuf,
};
use flutter_engine::{FlutterEngineArgs, FlutterEngine};

fn main() {
    flutter_engine::init();

    // This is flutter_assets direcotry you get with command `flutter build bundle`
    let assets_path = PathBuf::from(&env::var("ASSETS_PATH").unwrap());
    // This is a static file you get in flutter project
    let icu_data_path = PathBuf::from(&env::var("ICU_DATA_PATH").unwrap());

    let args = FlutterEngineArgs{
        assets_path: assets_path.to_string_lossy().into_owned(),
        icu_data_path: icu_data_path.to_string_lossy().into_owned(),
        title: String::from("Flutter Demo"),
        width: 800,
        height: 600,
    };

    let engine = FlutterEngine::new(args);
    engine.run();    // This blocks until window is closed
    engine.shutdown();
}

```

## demo
Check [this](https://github.com/gliheng/flutter-app-template) out for a runable demo.