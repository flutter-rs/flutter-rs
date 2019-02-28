# flutter-rs

[![Build status][ci-badge]][ci-url]
[![Crates.io][crates-badge]][crates-url]
[![flutter version][flutter-badge]][flutter-url]
[![Gitter chat][gitter-badge]][gitter-url]
[![MIT licensed][mit-badge]][mit-url]


Build flutter desktop app in dart & rust.

![demo ui](https://raw.githubusercontent.com/gliheng/flutter-rs/master/www/images/demo_ui.png "Flutter app demo screenshot")

# Install
- Install latest [Rust](https://www.rust-lang.org)
- Install libglfw:
    - Mac: `brew install glfw`
    - linux: `apt install libglfw3`
- Install [flutter sdk](https://flutter.io)

- In *flutter-app* project, set flutter sdk version in *Cargo.toml*

```
[package.metadata.flutter]
version = "5af435098d340237c5e3a69bce6aaffd4e3bfe84"
```

    This commit version id can be found in bin/internal/engine.version file in flutter sdk folder.

- Run `scripts/run.py` to get a running example.
    Note: The first run is going to take a while to download rust dependecies and flutter engine.

# Features:
- Support Hot reload
- MethodChannel, EventChannel
- Async runtime using tokio
- System dialogs
- Clipboard support
- Cross platform support (mac & linux)
- Support distribution format: (mac app, mac dmg)

# Contribution
To contribute to flutter-rs, please see [CONTRIBUTING](CONTRIBUTING.md).

[flutter-rs logo]: https://raw.githubusercontent.com/gliheng/flutter-rs/master/www/images/logo.svg
[flutter-badge]: https://img.shields.io/badge/flutter-v1.1.0-blueviolet.svg
[flutter-url]: https://flutter.dev/
[ci-badge]: https://ci.appveyor.com/api/projects/status/254owoouxk7t4w02?svg=true
[ci-url]: https://ci.appveyor.com/project/gliheng/flutter-rs
[gitter-badge]: https://badges.gitter.im/flutter-rs/community.svg
[gitter-url]: https://gitter.im/flutter-rs/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge
[crates-badge]: https://img.shields.io/crates/v/flutter-engine.svg
[crates-url]: https://crates.io/crates/flutter-engine
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: LICENSE-MIT
[demo-ui]: https://raw.githubusercontent.com/gliheng/flutter-rs/master/www/images/demo_ui.png