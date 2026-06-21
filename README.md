# flutter-rs

[![Crates.io][crates-badge]][crates-url]
[![flutter version][flutter-badge]][flutter-url]
[![Discord chat][discord-badge]][discord-url]
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

# Flutter engine library

The embedder links against `libflutter_engine.so`, which must match your Flutter
SDK's engine version.

- **JIT (debug)** builds are published by Google and can be downloaded directly
  (see `flutter-tools`, or grab `linux-x64/linux-x64-embedder.zip` from
  `storage.googleapis.com/flutter_infra_release`). Official builds no longer link
  the ICU data in, so pass `icudtl.dat` via `icu_data_path` /
  `FlutterEngineBuilder::with_icu_data_path`.
- **AOT (release/profile)** builds are *not* published for the generic embedder.
  Build one from source, matched to your installed SDK, with:

      scripts/build-engine.sh --mode release --out /path/to/your/app/.engine-release

  The script detects the SDK version, checks out the engine at the matching
  commit, runs `gclient sync` + `gn` + `ninja`, and copies the library out.

# Contribution
To contribute to flutter-rs, please see [CONTRIBUTING](CONTRIBUTING.md).

# ChangeLog
[CHANGELOG](CHANGELOG.md).

[flutter-rs logo]: https://raw.githubusercontent.com/flutter-rs/flutter-rs/master/www/images/logo.svg
[flutter-badge]: https://img.shields.io/badge/flutter-v3.44-blueviolet.svg
[flutter-url]: https://flutter.dev/
[discord-badge]: https://img.shields.io/discord/743549843632423053?label=discord
[discord-url]: https://discord.gg/WwdAE6p
[crates-badge]: https://img.shields.io/crates/v/flutter-engine.svg
[crates-url]: https://crates.io/crates/flutter-engine
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: LICENSE-MIT
[flutter-app-template]: https://user-images.githubusercontent.com/741807/72476798-5a99e280-37ee-11ea-9e08-b0175ae21ad6.png
[demo-ui]: https://raw.githubusercontent.com/flutter-rs/flutter-rs/master/www/images/demo_ui.png

