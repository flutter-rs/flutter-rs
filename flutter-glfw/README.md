# flutter-glfw

A [GLFW](https://www.glfw.org/) windowing backend for
[flutter-rs](https://github.com/flutter-rs/flutter-rs): it creates the window
and the OpenGL context, drives the event loop, and feeds pointer, key, scroll
and window-metrics events into [`flutter-engine`](../flutter-engine).

It is the only maintained backend in the workspace.

```toml
[dependencies]
flutter-glfw = "0.5"
```

`FlutterWindow::create` builds an engine with the GLFW OpenGL handler attached
and registers the plugins from [`flutter-plugins`](../flutter-plugins) that the
framework needs on a desktop embedder (text input, keyboard, mouse cursor,
accessibility, localization, navigation, platform, settings, system, lifecycle,
isolate, window, dialog). Note that the settings and lifecycle channels are
registered but never driven: nothing here tells the framework about the host's
brightness or that the app was resumed.
`FlutterWindow::run` then owns the calling thread, which is therefore the
engine's platform thread.

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
