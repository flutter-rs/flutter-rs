# Contributing guideline to flutter-rs

Thank you for your interest in contributing to flutter-rs! We have many areas that could use some help.

- Desktop intergration: We provide several methods to intergrate with native desktop UI, such as clipboard, dialog... but more is needed.
- Flutter Desktop GUI: Flutter is mainly for mobile. Desktop app need desktop widgets, such as context menu. Maybe a desktop app scarfold?
- Documentation is still lacking.
- Build intergration.

## Structure
This project is cargo workspace with multiple targets.

- `flutter-engine` is the library that make flutter run. It create a window using glfw and provide MethodChannel struct to iterop with flutter/dart. It also provide an async runtime using tokio.

    When an engine instance is run, a `platform_message_callback` is pass to flutter engine using C ffi. A registry is also created to listen to flutter MethodChannel calls. Various plugins is registered with the registry using `add_plugin` method. Later, when flutter request native implementation using `MethodChannel`, the callback previously passed as `platform_message_callback` is called, which select one plugin in the registry to handle the message.

- `flutter-app-demo` is a demo project that showcase various features of flutter-rs.
    - `flutter-app-demo/lib`: Dart code to create demo UI.
    - `flutter-app-demo/rust`: Rust code that uses flutter-engine to to start a flutter runtime.
- `flutter-download` is used by cargo to download libflutter at build time.
- `www` folder is various github website assets.

## Debugging
flutter-engine use `env_logger` to print logs. To see development logs, add environment variables when running `run.py`: `RUST_LOG=flutter_engine=trace,flutter_app_demo=trace ./scripts/run.py`

## Reference
- [Custom-Flutter-Engine-Embedders](https://github.com/flutter/flutter/wiki/Custom-Flutter-Engine-Embedders)
- [flutter engine](http://github.com/flutter/engine/)
- [Desktop Embedding for Flutter](https://github.com/google/flutter-desktop-embedding)