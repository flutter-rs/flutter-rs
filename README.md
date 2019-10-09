# flutter-rs

[![Appveyor status][appveyor-badge]][appveyor-url]
[![Travis status][travis-badge]][travis-url]
[![Crates.io][crates-badge]][crates-url]
[![flutter version][flutter-badge]][flutter-url]
[![Gitter chat][gitter-badge]][gitter-url]
[![MIT licensed][mit-badge]][mit-url]


Build flutter desktop app in dart & rust.

![demo ui](https://raw.githubusercontent.com/gliheng/flutter-rs/master/www/images/demo_ui.png "Flutter app demo screenshot")

# Install
- Install [Rust@^1.35.0](https://www.rust-lang.org)
- Python3
- LLVM: required by rust-bindgen
- Install additional requirements:
    - Mac: `brew install glfw`
    - linux: `apt install libglfw3`
    - ubuntu: `apt install libglfw3 libglfw3-dev libxinerama-dev libxcursor-dev libxi-dev ; ln -s /usr/lib/x86_64-linux-gnu/libGL.so.1 /usr/lib/libGL.so`
    - Windows: cmake is required to build glfw on windows
- Install [flutter sdk](https://flutter.io)
- Set flutter engine version. You can set this using any of the following methods.
    - If you have flutter cli in your PATH, you're set.
    - Set FLUTTER_ROOT environment variable to your flutter sdk path
    - Set FLUTTER_ENGINE_VERSION environment variable. This commit version id can be found in `bin/internal/engine.version` file in flutter sdk folder.

# Run flutter-app-demo example

- Run `scripts/init.py` to download flutter engine library and required python build dependencies.

- Run `scripts/run.py` to get a running example with flutter cli debugger attached.

- Run `scripts/build.py --release nsis|mac|dmg|snap` to build distribution format

# Features:
- Support Hot reload
- MethodChannel, EventChannel
- Async runtime using tokio
- System dialogs
- Clipboard support
- Cross platform support, Runs on mac, windows, linux
- Support distribution format: (windows NSIS, mac app, mac dmg, linux snap)

# Contribution
To contribute to flutter-rs, please see [CONTRIBUTING](CONTRIBUTING.md).

# ChangeLog
[CHANGELOG](CHANGELOG.md).

[flutter-rs logo]: https://raw.githubusercontent.com/gliheng/flutter-rs/master/www/images/logo.svg
[flutter-badge]: https://img.shields.io/badge/flutter-v1.9.1-blueviolet.svg
[flutter-url]: https://flutter.dev/
[appveyor-badge]: https://ci.appveyor.com/api/projects/status/254owoouxk7t4w02?svg=true
[appveyor-url]: https://ci.appveyor.com/project/gliheng/flutter-rs
[travis-badge]: https://travis-ci.com/gliheng/flutter-rs.svg?branch=master
[travis-url]: https://travis-ci.com/gliheng/flutter-rs
[gitter-badge]: https://badges.gitter.im/flutter-rs/community.svg
[gitter-url]: https://gitter.im/flutter-rs/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge
[crates-badge]: https://img.shields.io/crates/v/flutter-engine.svg
[crates-url]: https://crates.io/crates/flutter-engine
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: LICENSE-MIT
[demo-ui]: https://raw.githubusercontent.com/gliheng/flutter-rs/master/www/images/demo_ui.png

