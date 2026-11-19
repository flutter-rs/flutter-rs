# multiple_windows on flutter-glfw

Flutter's official `examples/multiple_windows` reference app, copied from the
Flutter 3.50 SDK, plus a flutter-rs runner.

The Dart UI (`lib/app/`, `lib/main.dart`) is unchanged. The only additions are:

- `lib/main_glfw.dart` — the entrypoint. It enables the framework's
  experimental windowing flag at runtime and installs `GlfwWindowingOwner`
  before calling the example's `main()`.
- `runner/` — a small Rust binary that starts one flutter-glfw app and lets
  Dart create every window.
- `run.sh` — builds the Dart bundle, fetches the matching generic embedder if
  needed, builds the runner and starts it.
- `run_macos.sh` — the same for macOS, where the engine's artifact is a
  `FlutterEmbedder.framework` and the app must use the software renderer.

## Running

```bash
./run.sh
```

On macOS:

```bash
FLUTTER_ROOT=$HOME/development/flutter-3.50.0 ./run_macos.sh
```

The macOS engine has no OpenGL backend, so the app uses Metal + Impeller by
default: the engine renders into `MTLTexture` backing stores and the compositor
presents them through each window's `CAMetalLayer`.
`FLUTTER_GLFW_RENDERER=software` forces the CPU renderer, which uploads each view
through the window's GL context; `FLUTTER_ENGINE_ARGS=--enable-impeller=false`
keeps Skia on the Metal path. The
script downloads the universal `FlutterEmbedder.framework` into `.engine/` when
`FLUTTER_ENGINE_LIB_DIR` does not already point at one, and runs the binary with
`DYLD_FRAMEWORK_PATH` set (the rpath emitted by `flutter-engine-sys` only covers
that crate's own targets).

Requirements: Flutter 3.50 on `PATH` (or `FLUTTER_ROOT` set), Rust, and the
system packages listed in the workspace's `CONTRIBUTING.md`. On a Wayland
session, use XWayland for the full demo so tooltips, popups and satellites can
be positioned:

```bash
FLUTTER_GLFW_PLATFORM=x11 ./run.sh
```

`FLUTTER_ENGINE_LIB_DIR` and `FLUTTER_ICU_DATA` override the downloaded engine
and ICU data; `LD_LIBRARY_PATH` is set by the script.

## What the example creates

- the main window;
- regular windows (`WindowController`);
- modeless and modal dialogs (`DialogWindowController`);
- tooltips and popups (`TooltipWindowController`, `PopupWindowController`);
- satellites (`SatelliteWindowController`), implemented as independent windows
  that follow their parent's position and are destroyed with it.

All windows are engine views of one isolate: the `WindowController` APIs talk
to `flutter_glfw_windowing`, which calls the Rust function table directly. The
first frame's 1×1 metrics event for the implicit view is what lets the app build
the tree that creates its first window.

## Notes

- Flutter gates the windowing APIs to its main channel; `main_glfw.dart` flips
  the framework's runtime `isWindowingEnabled` variable instead of requiring
  `--enable-windowing` at build time.
- Sizes and positions cross the ABI in logical pixels. The backend converts to
  and from GLFW's screen coordinates using the window content scale.
- Native Wayland does not let clients position top-level windows; X11/XWayland
  is required for tooltip/popup/satellite placement.
- Dialog modality: on X11 the backend sets `WM_TRANSIENT_FOR` and asks the WM
  for `_NET_WM_STATE_MODAL` (verified with Mutter), and independently drops
  parent input while a modal child exists, so modality survives on platforms
  whose WM has no such hint. Dialogs still close with their parent.
- Satellites keep an absolute offset from their parent and are marked floating,
  so fast parent moves no longer accumulate a gap and the satellite does not
  slide under the parent window.
