# flutter-engine

Safe Rust bindings to the [Flutter engine's embedder API][embedder], and the
core of [flutter-rs](https://github.com/flutter-rs/flutter-rs).

This crate owns the engine lifecycle (`FlutterEngineBuilder`, `FlutterEngine`),
the platform-message plumbing (`MethodChannel`, `MessageChannel`, the standard /
JSON / string codecs), the engine's task runner and the external-texture
registry. It is windowing-system agnostic: a backend such as
[`flutter-glfw`](../flutter-glfw) supplies the window, the GL context and the
input events, and [`flutter-plugins`](../flutter-plugins) supplies the system
channels the Flutter framework expects.

```toml
[dependencies]
flutter-engine = "0.5"
```

Two rules the API cannot enforce for you:

- Engine calls belong on the thread that created the engine (the *platform
  thread*). Use `FlutterEngine::run_on_platform_thread` from anywhere else.
- A panic unwinding into C aborts the process. The ten `extern "C"` callbacks
  this crate installs are panic-guarded, so a panicking handler is logged
  rather than fatal — which depends on `panic = "unwind"` staying set.

`gl-helpers` is the one optional feature: it pulls in `gl` and `image` for the
`TextureFrame` helpers that upload an `image::RgbaImage` to a GL texture.

## Linking

The engine library is neither shipped nor built here; see
[flutter-engine-sys](../flutter-engine-sys) and the
[workspace README](../README.md#flutter-engine-library) for how to obtain
`libflutter_engine` and put it on the link and loader paths.

## License

MIT — see [LICENSE](../LICENSE).

[embedder]: https://github.com/flutter/flutter/blob/main/engine/src/flutter/shell/platform/embedder/embedder.h
