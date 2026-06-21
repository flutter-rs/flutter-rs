# 0.5.0
- Update to the modern Flutter embedder API (Flutter 3.44 / engine struct_size-versioned structs).
- Regenerate `flutter-engine-sys` bindings with bindgen 0.72 against the current `embedder.h`; build now works with libclang 22 and without `llvm-config`.
- Zero-initialise embedder structs (`FlutterProjectArgs`, renderer/task-runner/pointer/window-metrics) so new engine fields no longer break the build.
- Add `FlutterEngineBuilder::with_icu_data_path`; official engine builds ship `icudtl.dat` separately instead of linking it in.
- `flutter-tools` now downloads the embedder from Flutter's official artifact storage (the `flutter-rs/engine-builds` repo is dead) and can locate `icudtl.dat` in the SDK cache.
- Bump all crates to edition 2021 and modern dependency versions (glfw 0.59, parking_lot 0.12, image 0.25, unic-locale 0.9, zip 2, indicatif 0.18, …).

# 0.3.0
- Code refactor thanks to Sophie Tauchert.
- Multi channel plugin support.
- List scrolling.
- Update flutter embedder.
- Add flutter-engine-sys crate to generate bindings using bindgen.
- flutter-download now download flutter engine of the same version as your flutter sdk.
- Support linux snap distribution.

# 0.2.0
- support windows
- nsis installer for windows
- new example UI

# 0.4.0
- use winit instead of glfw
- use custom flutter engine builds
- new cli tool `cargo-flutter`
- refactor engine to be window framework agnostic
- dropped tokio and libc dependencies
- move plugins to `flutter-plugins`
