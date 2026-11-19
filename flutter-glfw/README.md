# flutter-glfw

A [GLFW](https://www.glfw.org/) windowing backend for
[flutter-rs](https://github.com/flutter-rs/flutter-rs): one Flutter engine,
many native windows/views, a per-view OpenGL compositor, and a single event
loop that feeds pointer, key, scroll, focus and window-metrics events into
[`flutter-engine`](../flutter-engine).

It is the only maintained backend in the workspace. The Dart half of the
framework's windowing API is in [`dart/`](dart) (`flutter_glfw_windowing`), and
`../examples/multiple_windows` runs Flutter's official reference app on it.

```toml
[dependencies]
flutter-glfw = "0.5"
```

`FlutterDesktop::create_app` builds the one engine, its hidden master/resource
contexts and the plugins from [`flutter-plugins`](../flutter-plugins) that a
desktop embedder needs (text input, keyboard, mouse cursor, accessibility,
localization, navigation, platform, settings, system, lifecycle, isolate,
window, dialog). `FlutterApp::create_window` adds native windows and engine
views; `FlutterApp::run` owns the calling thread, which is the engine's platform
thread. The first window owns the implicit view (`0`), so `runApp` apps work;
later windows get `AddView` ids and are what the framework's `WindowController`
APIs drive.

Every visible window shares its GL objects with a hidden master context; the
engine renders each view into an offscreen framebuffer and the compositor blits
it into the window's context. The crate vendors a one-line patch to glfw-rs
0.63 (`vendor/glfw`) because upstream does not destroy windows created with
`create_shared`.

The macOS engine has no OpenGL backend, so there the app uses Metal by default:
the engine renders (through Impeller) into embedder-owned `MTLTexture` backing
stores and the compositor copies them into a per-view `CAMetalLayer` on the
shared command queue. `FLUTTER_GLFW_RENDERER=software` switches to the CPU
renderer, whose pixels are uploaded into the window's GL context before the
same blit; `--enable-impeller=false` keeps Skia on the Metal path. The official
macOS artifact is a universal `FlutterEmbedder.framework`.

### Multi-window from Dart

`dart/` implements `GlfwWindowingOwner`; the Rust side exposes a
`#[repr(C)]` function table whose address is passed as
`FlutterProjectArgs.engine_id`. The framework's windowing APIs
(`WindowController`, dialogs, tooltips, popups, satellites) work without a
platform channel and without exported C symbols. See
`../examples/multiple_windows/README.md`.

## Requirements

- Rust 1.95 or newer (the rest of the workspace builds on 1.88; glfw 0.63 uses
  `cfg_select!`).
- An engine library on the link and loader paths — see the
  [workspace README](../README.md#flutter-engine-library).
- GLFW's own build dependencies. `glfw-sys` links a system GLFW ≥ 3.4 when
  `pkg-config` finds one and otherwise builds the vendored sources with CMake,
  which on Linux needs the X11 and Wayland development packages listed in
  [CONTRIBUTING.md](../CONTRIBUTING.md#building-and-testing).

## License

MIT — see [LICENSE](../LICENSE).
