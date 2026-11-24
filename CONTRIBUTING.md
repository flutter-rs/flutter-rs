# Contributing guideline to flutter-rs

Thank you for your interest in contributing to flutter-rs! We have many areas
that could use some help.

- Reporting and fixing platform specific bugs.
- Desktop integration: we provide several plugins to integrate with native
  desktop UI, such as clipboard and dialogs, but more is needed.
- Flutter desktop GUI: Flutter is mainly for mobile. Desktop apps need desktop
  widgets, such as context menus.
- Documentation.

## Structure

This project is a cargo workspace of five crates, layered bottom-up:

- `flutter-engine-sys` is the ffi layer for the Flutter engine's embedder C
  API. It generates bindings with bindgen from the vendored
  `flutter-engine.h`, and declares the link against `libflutter_engine`
  without shipping or building it.

- `flutter-engine` is the library that makes Flutter run. It owns the engine
  lifecycle and provides `MethodChannel`/`MessageChannel` to interop with
  Flutter and Dart.

    When an engine instance is run, a `platform_message_callback` is passed to
    the Flutter engine using C ffi. A registry listens to Flutter
    `MethodChannel` calls. Various plugins are registered with the registry
    using the `add_plugin` method. When Flutter makes a native platform
    request, the callback is called and processed by a registered plugin.

- `flutter-plugins` implements the Flutter system channels (text input,
  keyboard, mouse cursor, accessibility, localization, navigation, platform,
  settings, window, dialog, isolate) on top of `flutter-engine`'s channel
  registry. An embedder has to register them; `flutter-glfw` does.

- `flutter-glfw` creates one engine, any number of GLFW windows/views, the GL
  contexts and the per-view compositor, runs the single event loop, and
  interfaces with the `flutter-engine` crate. `flutter-glfw/dart` is the
  matching Dart `WindowingOwner`. It is the only maintained windowing backend;
  `vendor/glfw` is a one-line patch to glfw 0.63 for `create_shared` windows.

    (`flutter-winit` is an unmaintained winit-based alternative. Its sources are
    still tracked, but it has not been ported to winit 0.30 / glutin 0.32, does
    not compile, and is `exclude`d from the workspace in the root `Cargo.toml`.)

- `flutter-tools` locates the Flutter SDK, reports the engine revision it is
  pinned to, downloads the matching `libflutter_engine` from Flutter's artifact
  storage, and finds `icudtl.dat` in the SDK cache.

`flutter-glfw/src/keymap.rs` is partly generated. Regenerate the table from the
installed SDK with `scripts/gen-keymap.py` (it rewrites the file in place and
preserves the hand-maintained tail); `scripts/gen-keymap.py --check` reports
drift without writing.

## Building and testing

On Debian/Ubuntu, the system packages are:

```bash
sudo apt-get install -y cmake pkg-config libclang-dev \
  libx11-dev libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev libxext-dev \
  libwayland-dev libxkbcommon-dev
```

`libclang` is for bindgen, which runs from flutter-engine-sys's build script
even under `cargo check`. The rest is what GLFW 3.4 needs when `glfw-sys` finds
no system GLFW >= 3.4 through `pkg-config` and builds its vendored copy: X11
with the Xrandr/Xinerama/Xcursor/Xi/Xext extensions, plus wayland-client/
cursor/egl and xkbcommon for the Wayland backend.

`cargo check`, `cargo clippy` and `cargo doc` need no engine library — the
workspace has no bin or example target, so nothing links. `cargo test` does;
see [Flutter engine library](README.md#flutter-engine-library), then:

```bash
export FLUTTER_ENGINE_LIB_DIR=/path/to/engine   # link time
export LD_LIBRARY_PATH=/path/to/engine          # run time
cargo test --workspace
```

The minimum supported Rust version is 1.88, declared in `[workspace.package]`.
flutter-glfw overrides it with 1.95 in its own manifest, because glfw 0.63 uses
`cfg_select!`; `cargo check --workspace` therefore needs 1.95.

## Checks CI enforces

[`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs these; run them
before opening a pull request.

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

Lints are centralised in `[workspace.lints]` in the root `Cargo.toml`; each
member inherits them with `[lints] workspace = true`, so add a lint there
rather than per crate. `clippy::correctness` is `deny`. The exceptions are
`missing_docs` and the `unsafe`-documentation lints, which are crate-root
`#![warn(..)]` attributes because flutter-engine-sys's generated bindings
cannot satisfy them.

Dependencies are checked with [cargo-deny](https://github.com/EmbarkStudios/cargo-deny)
against [`deny.toml`](deny.toml):

```bash
cargo install cargo-deny --locked
cargo deny check advisories licenses bans sources
```

CI additionally compiles the workspace on macOS and checks both MSRV floors. It
does not run the tests on macOS: the Apple engine artifact is
`FlutterEmbedder.framework`, which `flutter-engine-sys` now links as a
framework, but the runtime path is unverified.

`examples/multiple_windows` is a standalone Flutter project plus a standalone
Rust runner (its own `[workspace]`), so it is not built by the workspace gate.
Run it with `examples/multiple_windows/run.sh`; it needs Flutter 3.50 and the
generic embedder for the SDK's engine revision.

A Windows-gnu cross-check is
`cargo check --workspace --target x86_64-pc-windows-gnu --no-default-features`
(with mingw-w64, clang and cmake). The `dialog` feature pulls tinyfiledialogs,
whose `#include <ShellScalingApi.h>` only resolves case-insensitively on a
native Windows filesystem; native/MSVC builds and the feature-off cross-check
are fine.

## Testing notes

There are no integration tests and nothing here drives a live engine, because
that needs a window, a GL context and a real `libflutter_engine`. Unit tests
therefore cover the pure logic — codecs, the text-editing model, index and key
mapping, error display, the pointer-button mask — and the engine-facing code is
held together by SAFETY comments, panic guards on every `extern "C"` callback,
and review. If you change engine-facing code, say in the pull request what you
ran it against.

`cargo miri test` is **not** run. Miri executes no foreign functions and cannot
`dlopen` `libflutter_engine`, so neither flutter-engine-sys's `link` probe nor
any engine-calling path can run under it. The pure-Rust slice (the codecs) is
miri-able in principle and would be the place to start if anyone wants it.

## Reference

- [Custom Flutter Engine Embedders](https://github.com/flutter/flutter/blob/main/docs/engine/Custom-Flutter-Engine-Embedders.md)
- [`embedder.h`](https://github.com/flutter/flutter/blob/main/engine/src/flutter/shell/platform/embedder/embedder.h),
  the C API this workspace wraps
- [Desktop Embedding for Flutter](https://github.com/google/flutter-desktop-embedding) (archived 2024)
