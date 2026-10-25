# flutter-rs

[![Crates.io][crates-badge]][crates-url]
[![flutter version][flutter-badge]][flutter-url]
[![MIT licensed][mit-badge]][mit-url]

Build Flutter desktop apps in Dart and Rust.

![flutter-app-template][flutter-app-template]

## Status

This workspace targets the modern Flutter embedder API and is developed against
Flutter **3.44 – 3.47.x**; the vendored `flutter-engine-sys/flutter-engine.h` is
Flutter 3.47.8's `flutter_embedder.h` verbatim, and the test suite is run against
both 3.47.8's engine (`cf636b1d…`) and 3.44.2's (`77e2e947…`).

The newest release on crates.io is **0.3.5 (2019)**. Everything here is the
unreleased 0.5.0 work; see [CHANGELOG.md](CHANGELOG.md).

`cargo-flutter`, which older copies of these instructions (and the app
template's own README) tell you to install, has not been released since
January 2020 and no longer compiles. There is no replacement CLI: you drive
`flutter` and `cargo` yourself, as described below.

## Crates

| Crate | What it is |
| --- | --- |
| [`flutter-engine-sys`](flutter-engine-sys) | Raw bindgen bindings to the embedder C API. |
| [`flutter-engine`](flutter-engine) | Safe engine lifecycle, platform-message channels, codecs, task runner, textures. |
| [`flutter-plugins`](flutter-plugins) | The `flutter/*` system channels: text input, keyboard, clipboard, cursor, locales, settings, … |
| [`flutter-glfw`](flutter-glfw) | The windowing backend: GLFW window, GL context, event loop. The only maintained one. |
| [`flutter-tools`](flutter-tools) | Finds the Flutter SDK and downloads the matching engine library. |

`flutter-winit` is still in the tree but is unmaintained, does not compile, and
is `exclude`d from the workspace.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) 1.88 or newer for
  flutter-engine-sys / flutter-engine / flutter-plugins / flutter-tools;
  flutter-glfw declares 1.95, because glfw 0.63 uses `cfg_select!`.
- A [Flutter SDK](https://flutter.dev/) on `PATH` (or `FLUTTER_ROOT` set),
  3.44 – 3.47.x. Newer SDKs may work — the embedder ABI has not moved since
  3.44 — but have not been tried.
- On Linux, the system packages listed under
  [Building and testing](CONTRIBUTING.md#building-and-testing).

## Flutter engine library

The embedder links against `libflutter_engine` (`.so` / `.dylib` /
`flutter_engine.dll`), which must match your Flutter SDK's engine revision.
Nothing in this repository ships or builds it for you.

### JIT (debug) — downloaded

Google publishes a JIT build of the generic embedder for each engine revision.
`flutter-tools` fetches and caches it:

```rust
use flutter_tools::{Build, Engine, Flutter};

let flutter = Flutter::auto_detect()?;
let engine = Engine::new(
    flutter.engine_version()?,
    "x86_64-unknown-linux-gnu".to_owned(),
    Build::Debug,
)
.with_realm(flutter.engine_realm());

engine.download()?;
println!("{}", engine.library_path()?.display());
```

Or by hand, which is what CI does:

```bash
hash="$(cat "$FLUTTER_ROOT/bin/cache/engine.stamp" 2>/dev/null \
  || cat "$FLUTTER_ROOT/bin/internal/engine.version")"
curl -fL -o embedder.zip \
  "https://storage.googleapis.com/flutter_infra_release/flutter/$hash/linux-x64/linux-x64-embedder.zip"
unzip embedder.zip -d ~/.flutter-engine   # libflutter_engine.so lands here
```

Official builds no longer link the ICU data in, so you must also pass
`icudtl.dat` (`Flutter::icudtl_path`, or
`$FLUTTER_ROOT/bin/cache/artifacts/engine/*/icudtl.dat`) to
`FlutterEngineBuilder::with_icu_data_path` /
`FlutterDesktop::create_window_with_icu`.

### AOT (release / profile) — built from source

AOT engines are *not* published for the generic embedder. Build one matched to
your SDK:

```bash
scripts/build-engine.sh --mode release --out /path/to/your/app/.engine-release
```

The script detects the SDK version, checks out `flutter/flutter` at the matching
commit, runs `gclient sync` + `gn` + `ninja`, and copies the library out. Run
`scripts/build-engine.sh --help` for the options. Expect hours and tens of GB on
the first run.

### Telling the toolchain where it is

- **Link time:** set `FLUTTER_ENGINE_LIB_DIR=<dir>` (flutter-engine-sys's build
  script turns it into a `-L`), or pass `-L <dir>` yourself via `RUSTFLAGS`.
  The two are interchangeable and compose.
- **Run time:** put the same directory on `LD_LIBRARY_PATH` (Linux),
  `DYLD_LIBRARY_PATH` (macOS) or `PATH` (Windows).

## Writing an app

The Dart half is an ordinary Flutter project; build its assets with
`flutter build bundle`. The Rust half opens the window:

```rust
use flutter_glfw::window::{WindowArgs, WindowMode};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The thread that calls `init` becomes the engine's platform thread.
    let mut desktop = flutter_glfw::init()?;

    let window = desktop.create_window_with_icu(
        &WindowArgs {
            width: 1280,
            height: 720,
            title: "My app",
            mode: WindowMode::Windowed,
        },
        PathBuf::from("build/flutter_assets"), // `flutter build bundle` output
        PathBuf::from("icudtl.dat"),           // from the SDK, or flutter-tools
        Vec::new(),                            // extra engine switches
    )?;

    // Owns this thread until the window closes.
    window.run(None, None)?;
    Ok(())
}
```

[flutter-app-template](https://github.com/flutter-rs/flutter-app-template) is a
worked example of the project layout, but ignore its `cargo flutter` commands.

Hot reload and distribution packaging (appimage / snap / nsis) came from
`cargo-flutter` and are **currently unsupported**.

## Building this repository

```bash
cargo build --workspace
cargo test --workspace     # needs an engine library, see above
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full check list that CI enforces.

## Contribution

To contribute to flutter-rs, please see [CONTRIBUTING](CONTRIBUTING.md).

## ChangeLog

[CHANGELOG](CHANGELOG.md).

## License

MIT — see [LICENSE](LICENSE).

[flutter-badge]: https://img.shields.io/badge/flutter-v3.44%20--%20v3.47-blueviolet.svg
[flutter-url]: https://flutter.dev/
[crates-badge]: https://img.shields.io/crates/v/flutter-engine.svg
[crates-url]: https://crates.io/crates/flutter-engine
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: LICENSE
[flutter-app-template]: https://user-images.githubusercontent.com/741807/72476798-5a99e280-37ee-11ea-9e08-b0175ae21ad6.png
