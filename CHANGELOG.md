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
