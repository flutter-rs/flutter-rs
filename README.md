# flutter-rs

[![Crates.io][crates-badge]][crates-url]
[![flutter version][flutter-badge]][flutter-url]
[![Gitter chat][gitter-badge]][gitter-url]
[![MIT licensed][mit-badge]][mit-url]

Build flutter desktop app in dart & rust.

![flutter-app-template][flutter-app-template]

# Get Started

## Install requirements

- [Rust](https://www.rust-lang.org/tools/install)

- [flutter sdk](https://flutter.io)

## Develop
- install the `cargo` `flutter` command

    `cargo install cargo-flutter`
    
- create your new project from the template

    `git clone https://github.com/flutter-rs/flutter-app-template`

- To develop with cli hot-reloading:

    `cd flutter-app-template`
    
    `cargo flutter run`

## Distribute
- To build distribution, use:
    `cargo flutter --format appimage build --release`

# Contribution
To contribute to flutter-rs, please see [CONTRIBUTING](CONTRIBUTING.md).

# ChangeLog
[CHANGELOG](CHANGELOG.md).

[flutter-rs logo]: https://raw.githubusercontent.com/flutter-rs/flutter-rs/master/www/images/logo.svg
[flutter-badge]: https://img.shields.io/badge/flutter-v1.9.1-blueviolet.svg
[flutter-url]: https://flutter.dev/
[gitter-badge]: https://badges.gitter.im/flutter-rs/community.svg
[gitter-url]: https://gitter.im/flutter-rs/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge
[crates-badge]: https://img.shields.io/crates/v/flutter-engine.svg
[crates-url]: https://crates.io/crates/flutter-engine
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: LICENSE-MIT
[flutter-app-template]: https://user-images.githubusercontent.com/741807/72476798-5a99e280-37ee-11ea-9e08-b0175ae21ad6.png
[demo-ui]: https://raw.githubusercontent.com/flutter-rs/flutter-rs/master/www/images/demo_ui.png

