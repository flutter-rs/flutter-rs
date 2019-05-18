**flutter-engine is a library to make desktop apps in flutter and rust**

## flutter-engine in action

```rust
const ASSETS_PATH: &str = "../build/flutter_assets";
const ICU_DATA_PATH: &str = "./assets/icudtl.dat";

fn main() {
    let mut engine = flutter_engine::init().unwrap();
    engine
        .create_window(
            &flutter_engine::WindowArgs {
                height: 1200,
                width: 1800,
                title: "Flutter App Demo",
                mode: flutter_engine::WindowMode::Windowed,
                bg_color: (255, 255, 255),
            },
            ASSETS_PATH.to_string(),
            ICU_DATA_PATH.to_string(),
            vec![],
        )
        .unwrap();
    engine.run_window_loop(None, None);
}
```

## demo
Check [this](https://github.com/gliheng/flutter-app-template) out for a runable demo.