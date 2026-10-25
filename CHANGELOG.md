# Changelog

All notable changes to the flutter-rs workspace. Versions are shared by all
five crates. Dates are the dates of the corresponding git tags.

## 0.5.0 — unreleased

The first work on this project in six years. It ports the embedder to the
modern Flutter engine API (developed against **Flutter 3.44 – 3.47.x**), moves
the workspace to edition 2024, removes essentially every panic from the FFI
boundary, fixes two memory-safety bugs in the standard codec, and replaces the
dead `cargo-flutter` workflow with plain `cargo`.

This is a hard break with 0.3.5: no 0.3.x code compiles unchanged. `flutter-rs`
0.4.0 was never published to crates.io, so for crates.io users this supersedes
0.3.5 directly.

Highlights:

- **Nothing in the engine callbacks aborts the process any more.** Every
  `extern "C"` entry point that can reach user code is panic-guarded, the
  decode / respond / argument paths return errors instead of unwrapping, and
  engine results are checked and logged instead of discarded.
- **Two memory-safety fixes in the message path.** `Reader::read_string` read
  out of bounds on a truncated message (reproduced: 198 bytes of adjacent heap
  from a 1-byte buffer), and the reply-callback user-data pointer was boxed and
  reclaimed at two different types (reproduced: SIGSEGV). The whole `Reader` is
  now bounds-checked, and the JSON and string codecs no longer use
  `from_utf8_unchecked`.
- **`Float32List` is decoded.** Wire tag 14 previously decoded as
  `Value::Null` with its payload left unread, silently corrupting every later
  value in the same message.
- **Text editing counts UTF-16 code units, as Dart does.** Selections, carets
  and `updateEditingState` were wrong for any text containing an emoji or other
  non-BMP character, and three operations panicked on a freshly focused field.
- **Supply chain.** `async-std` (RUSTSEC-2025-0052, unmaintained) is gone, `zip`
  is on 9.x (RUSTSEC-2026-0367), and `cargo deny check advisories licenses bans
  sources` is clean and runs in CI.

### Breaking changes

#### Workspace

- Edition 2024 and `resolver = "3"`; the declared MSRV is Rust **1.88**
  (`rust-version` in `[workspace.package]`). `flutter-glfw` declares **1.95**
  in its own manifest, because glfw 0.63 uses `cfg_select!`, so a
  `--workspace` build needs 1.95.
- All five crates are version 0.5.0, and the internal path dependencies now
  carry a `version`, so the workspace is publishable again. Dependencies and
  lints are centralised in `[workspace.dependencies]` and `[workspace.lints]`.
- `flutter-winit` is `exclude`d from the workspace. Its sources are still
  tracked, but it has not been ported to winit 0.30 / glutin 0.32 and does not
  compile. `flutter-glfw` is the only maintained backend.
- `flutter-plugins`' `tinyfiledialogs` dependency is now optional behind a
  default `dialog` feature. `flutter-glfw` enables it explicitly.
- `flutter-engine`'s `gl-helpers` feature uses `dep:` syntax
  (`["dep:gl", "dep:image"]`), so `--features gl` and `--features image` are
  rejected instead of half-configuring the crate.
- Dependency majors: bindgen 0.73, dirs 7, glfw 0.63 (X11 + Wayland +
  raw-window-handle 0.6), libloading 0.9, which 8, zip 9, `sys-locale` in place
  of the long-dead `locale_config`, plus a new `thiserror` 2 dependency in
  flutter-engine, flutter-plugins, flutter-glfw and flutter-tools.
- `[profile.release]` sets `lto = "thin"` and `codegen-units = 1`. `panic` is
  deliberately left at `unwind`: the FFI panic guards depend on it.

#### flutter-engine — engine core

- `FlutterEngine::run` returns `Result<(), RunError>` instead of
  `Result<(), ()>`, and reports `RunError::NotPlatformThread` instead of
  panicking when called off the platform thread.
- `CreateError` is `#[non_exhaustive]`; `NoHandler` is replaced by
  `NoOpenGlHandler` and `NoPlatformHandler`, and `InteriorNul` and
  `EngineInitialize(i32)` are new. Its `Display` messages are lowercase and no
  longer end in a newline or a period.
- `FlutterOpenGLHandler` implementations must now be `Sync` as well as `Send`,
  and `FlutterEngineBuilder::with_opengl` requires
  `H: FlutterOpenGLHandler + Send + Sync + 'static`. The engine makes the
  context current on its raster *and* its IO thread, so the old bound was
  unsound.
- `MessageError::RustError` and `MethodCallError::RustError` carry
  `Box<dyn Error + Send + Sync>` (new alias `flutter_engine::error::BoxError`),
  so both error types are now `Send + Sync`; `MessageError::from_error` and
  `MethodCallError::from_error` require `T: Error + Send + Sync + 'static`.
- All five error enums in `flutter_engine::error` are `#[non_exhaustive]` and
  implement `Error::source()` instead of the deprecated `Error::cause()`, so
  the wrapped error is visible to `anyhow`, `eyre` and `Error::sources()`.
- `impl Into<MethodCallResult> for MethodCallError` is replaced by
  `impl From<MethodCallError> for MethodCallResult`. Existing `.into()` call
  sites are unaffected.
- `FlutterEngine::arguments` returns `&[String]` instead of `&Vec<String>`, and
  `FlutterEngineWeakRef::ptr_equal` takes `&Self` instead of `Self`.
- `FlutterEngine::send_pointer_event` takes `buttons: impl Into<PointerButtons>`
  — the cumulative mask of buttons held — instead of a single
  `FlutterPointerMouseButtons`, plus a new final argument
  `pan_zoom: Option<ffi::PanZoom>`. Call sites passing a
  `FlutterPointerMouseButtons` still compile; every call site needs the new
  argument.
- `ffi::FlutterPointerPhase`, `ffi::FlutterPointerDeviceKind` and
  `ffi::FlutterPointerSignalKind` are `#[non_exhaustive]`. `KeyEventType` and
  `FlutterPointerMouseButtons` stay exhaustive; a sixth mouse button is
  expressed through `PointerButtons::from_bits`.
- `tasks::TaskRunner::new` and `tasks::TaskRunner::execute_tasks` are
  `pub(crate)`. Constructing a `TaskRunner` from outside only ever produced one
  that panicked on first use. `TaskRunnerHandler` stays public.

#### flutter-engine — channels and codecs

- `MethodCodec::decode_method_call`, `MethodCodec::decode_envelope` and
  `MessageCodec::decode_message` return `Result<_, codec::DecodeError>` instead
  of `Option<_>`, so a caller can tell a truncated message from an unknown type
  tag from the wrong codec.
- `MethodCall::args` returns `Result<T, error::ValueError>` instead of `T`, and
  so does `Message::value`. Wrong arguments from Dart are now reported rather
  than aborting the process.
- `MethodCall::method` returns `&str` instead of `&String`.
- `codec::Value` has a new `F32List(Vec<f32>)` variant and is
  `#[non_exhaustive]`, so the next wire-format addition is not a breaking
  change.
- `codec::value::VecExt::push_as_value` returns
  `Result<(), codec::value::Error>` instead of panicking on a value serde
  cannot represent. `codec::value::Error` is `#[non_exhaustive]`, implements
  `Error::source()`, and its `Display` text is lowercase with no trailing
  period.
- `PlatformMessageResponseHandle::new` takes `&FlutterEngine` and returns
  `Option<Self>`: it checks the embedder result and the out-parameter.
- `impl Into<PlatformMessageResponseHandle> for *const FlutterPlatformMessageResponseHandle`
  and `impl Into<FlutterPlatformMessage> for PlatformMessage` are **removed**.
  Both were *safe* public conversions that dereferenced caller-supplied raw
  pointers and handed back values with unbounded lifetimes; they are replaced
  by a `pub(crate) unsafe fn from_raw` and a `pub(crate) into_raw`.
- `ChannelRegistry::handle` and `ChannelRegistry::with_channel` are replaced by
  `channel(&str) -> Option<Arc<dyn Channel>>` and `reject(PlatformMessage)`, so
  a caller can drop the registry's lock before dispatching into user code.
- `flutter_engine::channel::event_channel` is **deleted**. It had been excluded
  from the module tree since 0.3 and no longer compiled. Push events to Dart
  with `FlutterEngine::post_message` inside `run_on_platform_thread`.

#### flutter-plugins

- `SettingsPlugin::start_message` now takes the three settings the framework
  requires on every `flutter/settings` message —
  `start_message(text_scale_factor: f64, use_24_hour_format: bool, brightness: PlatformBrightness)`
  — and returns `SettingsMessage<'_>`. A partially populated message made the
  Dart side throw.
- `textinput::TextEditingState` is now public (`textinput::text_editing_state`
  is a public module and the type is re-exported), so the state
  `TextInputPlugin::with_state` hands out can be named and stored. Its fields
  stay private. Its offsets are **UTF-16 code-unit offsets**, as Dart counts
  them, not Rust `char` indices.
- `TextEditingState::from(Value) -> Option<Self>` is **removed**: it was dead,
  swallowed the deserialization error, and collided with the `From` convention.
- `platform::MimeError` is a `thiserror`-derived unit struct. It is still
  constructible as `MimeError`, but its `Display` is now
  "unsupported clipboard mime type" (was "Mime error").
- `flutter_plugins::keyevent` is `#[deprecated]`. `flutter/keyevent` is the
  legacy `RawKeyboard` protocol, which current Flutter no longer sends; send a
  `FlutterKeyEvent` through `FlutterEngine::send_key_event` instead.
- `IsolatePlugin::new`'s bound is `F: FnOnce() + 'static + Send` (was the
  equivalent `FnOnce() -> ()`).

#### flutter-glfw

- `WindowArgs::width` and `WindowArgs::height` are `u32` instead of `i32`, so a
  negative window size is unrepresentable rather than silently becoming a huge
  `u32`.
- `window::CreateError` implements `std::error::Error` (via `thiserror`), is
  `#[non_exhaustive]`, and has a new `Engine(flutter_engine::CreateError)`
  variant with a `From` impl, so `FlutterWindow::create` reports an engine
  failure instead of panicking. Its `Display` messages are lowercase and it no
  longer derives `Copy`, `Clone`, `Ord`, `PartialOrd` or `Hash`.
- `FlutterWindow::run` returns `Result<(), window::RunError>` instead of
  `Result<(), ()>`, so the embedder's reason for refusing to start reaches the
  caller.
- `FlutterWindow` now implements `Drop`, which shuts the engine down if it is
  still running and the dropping thread is the platform thread.
  `FlutterWindow::shutdown` still works and is equivalent to dropping.
- The dead `draw` module file is removed. It was never in the module tree.

#### flutter-tools

- `Error` is a `thiserror` enum and is `#[non_exhaustive]`; all messages are
  lowercase with no trailing punctuation, and every wrapped variant reports its
  cause through `source()`.
- `Error::DownloadNotFound` is a struct variant
  `DownloadNotFound { version: String, url: String }`. It used to format the
  literal string "missing" into the message and point at the
  `flutter-rs/engine-builds` releases page, which has published nothing since
  2021.
- `Engine::library_name`, `Engine::download_url`, `Engine::engine_dir` and
  `Engine::library_path` return `Result<_, Error>`. They previously panicked on
  an unrecognised target triple or an undeterminable cache directory.
- `Engine::download` returns `Err(Error::AotEngineNotPublished)` for
  `Build::Release` and `Build::Profile`. Flutter publishes no AOT build of the
  generic embedder, and the previous behaviour silently installed a JIT engine
  into a directory named `release`.
- `Flutter::icudtl_path` returns `Error::IcuDataNotFound` instead of reusing
  `Error::DownloadNotFound`.
- `Engine::dart` is `#[deprecated]`: the published embedder archives contain no
  Dart binary, so it can only ever return `Err(Error::DartNotFound)`. Use the
  new `Flutter::dart`.

#### Tooling

- `scripts/gen-keymap.py` changed from
  `scripts/gen-keymap.py [FLUTTER_ROOT] > flutter-glfw/src/keymap.rs` to
  `scripts/gen-keymap.py [-o OUTPUT] [--check] [FLUTTER_ROOT]`. It rewrites
  `flutter-glfw/src/keymap.rs` in place, regenerating only the generated table
  and keeping the hand-maintained remainder; `--check` reports drift without
  writing and `-o -` prints to stdout.

### Added

#### flutter-engine

- `#[non_exhaustive] enum RunError { NotPlatformThread, ShutDown, Engine(i32) }`,
  and `CreateError: From<std::ffi::NulError>`.
- `FlutterEngine::is_shut_down`, and `FlutterEngine::shutdown` is idempotent: a
  second call logs a warning and does nothing, and every other engine call
  becomes a logged no-op afterwards.
- `FlutterEngine::update_locales(&[(String, String, String, String)])`, wrapping
  `FlutterEngineUpdateLocales`. This is the only API that feeds
  `PlatformDispatcher.locales`; an empty code is reported to the framework as
  absent.
- `ffi::PointerButtons`, an `i64` bit-mask newtype (`NONE`, `PRIMARY`,
  `SECONDARY`, `MIDDLE`, `BACK`, `FORWARD`, and `STYLUS_CONTACT` /
  `STYLUS_PRIMARY` / `STYLUS_SECONDARY`, which alias the first three) with
  `bits`, `from_bits`, `is_empty`, `contains`, `BitOr`, `BitOrAssign`,
  `Default` and `From<FlutterPointerMouseButtons>`.
- `ffi::PanZoom { pan: (f64, f64), scale: f64, rotation: f64 }`, with a
  `Default` of pan `(0.0, 0.0)`, scale `1.0`, rotation `0.0`.
- `ffi::FlutterPointerPhase` gained `PanZoomStart`, `PanZoomUpdate` and
  `PanZoomEnd`; `ffi::FlutterPointerDeviceKind` gained `Stylus`,
  `InvertedStylus` and `Trackpad`; `ffi::FlutterPointerSignalKind` gained
  `ScrollInertiaCancel` and `Scale`.
- `#[non_exhaustive] enum codec::DecodeError { Truncated, UnknownType(u8), WrongType { expected }, NotUtf8, Json(serde_json::Error) }`,
  with `Display` and `Error::source`.
- `Message::raw_value() -> &Value`, the counterpart of `MethodCall::raw_args`.
- `codec::JsonMethodCodec`, `codec::StandardMethodCodec` and `codec::StringCodec`
  are re-exported, so the types of `JSON_CODEC` / `STANDARD_CODEC` /
  `STRING_CODEC` can be named.
- `FlutterEngineBuilder` implements `Default`; `new` and all six `with_*`
  methods are `#[must_use]`, as are the pure getters on `FlutterEngine`,
  `FlutterEngineWeakRef` and `Texture`. `Debug` is implemented for
  `FlutterEngine`, `FlutterEngineWeakRef`, `FlutterEngineBuilder`, `TaskRunner`,
  `Texture`, `TextureFrame`, `PluginRegistrar`, `MethodCall`, `Message`,
  `MethodChannel`, `MessageChannel`, `ChannelRegistry`, `MethodCallResult`,
  `channel::MethodError`, `codec::value::Deserializer` and the three codec
  types.
- Crate-level and per-item documentation, with `# Panics` and `# Errors`
  sections on the public API; `missing_docs`, `missing_debug_implementations`,
  `clippy::undocumented_unsafe_blocks` and `clippy::missing_safety_doc` are
  enabled crate-wide and clean.

#### flutter-plugins

- The `flutter/platform` plugin answers `Clipboard.hasStrings`, which is what
  the framework polls to decide whether to offer Paste in the text selection
  toolbar. It was silently not-implemented.
- `TextEditingState::text() -> &str`; `get_selected_text` is `#[must_use]`.
- `SettingsMessage` is
  `#[must_use = "settings are only delivered by calling .send()"]`, so dropping
  a builder chain without `.send()` warns.
- Every public item is documented, the crate has crate-level documentation, and
  `#![warn(missing_docs)]` keeps it that way.

#### flutter-glfw

- `#[non_exhaustive] enum window::RunError { Engine(flutter_engine::RunError) }`,
  implementing `Error` with `From<flutter_engine::RunError>`.
- `FlutterWindow::engine`, `window`, `create_texture` and `FlutterDesktop::glfw`
  are `#[must_use]`; the crate's public API is documented.

#### flutter-tools and flutter-engine-sys

- Error variants `IcuDataNotFound`, `UnsupportedTarget(String)`,
  `AotEngineNotPublished` and `NoCacheDir`.
- `Engine::with_realm(String) -> Self` and `Flutter::engine_realm() -> String`,
  so SDKs pinned to a presubmit artifact realm resolve. `Engine::new`'s
  signature is unchanged.
- `Engine::download_url` honours `FLUTTER_STORAGE_BASE_URL`, matching the
  `flutter` tool, so official mirrors work.
- `aarch64-pc-windows-msvc` is supported, mapping to the `windows-arm64`
  artifact and `flutter_engine.dll`.
- `Flutter::dart() -> Result<PathBuf, Error>`, which finds the SDK's own
  `bin/cache/dart-sdk/bin/dart`.
- `Flutter` derives `Clone, Debug, Eq, PartialEq`; the crate has module docs and
  `#![warn(missing_docs)]`, and every public item is documented.
- flutter-engine-sys understands a new build-time environment variable
  `FLUTTER_ENGINE_LIB_DIR`. Pointing it at a directory containing the engine
  library makes `build.rs` emit a `cargo::rustc-link-search` for it, plus an
  rpath on non-Windows targets so this package's own test binaries load it
  without `LD_LIBRARY_PATH`. The existing `RUSTFLAGS="-L <dir>"` workflow keeps
  working and the two compose.
- flutter-engine-sys has crate-level docs documenting the linking contract.

### Fixed

#### Panics and aborts at the FFI boundary

- The ten engine callbacks that dispatch into user code, the platform-message
  reply callback, the texture destruction callback and a posted render-thread
  task are panic-guarded. A panicking channel, GL or task handler is logged and
  the engine is told the call failed, instead of aborting the process. (This is
  why `[profile.release]` keeps `panic = "unwind"`.)
- `MethodChannel::invoke_method`, `invoke_method_with_result`,
  `MessageChannel::send` and `send_with_result` no longer panic when called off
  the platform thread: they encode on the calling thread and hop to the platform
  thread to send (inline when already there, so ordering is unchanged for an
  on-thread caller).
- `MethodCall::respond` and `Message::respond` log and return instead of
  panicking when Dart is not waiting for a reply, and a reply value that cannot
  be encoded now produces an error envelope rather than panicking — so the
  engine's response handle is always answered.
- Decoding a reply envelope of zero bytes yields
  `MethodCallResult::NotImplemented` (the framework's "no plugin handled this")
  in both the standard and the JSON codec. It used to panic.
- `plugins::PluginRegistrar` no longer panics: it uses a non-poisoning lock,
  logs a plugin-name collision instead of unwrapping a failed downcast, and
  warns when `add_plugin` replaces an existing plugin. `register_channel`
  likewise warns when it replaces a channel of the same name.
- `TextEditingState` no longer panics on an offset it did not expect: Flutter's
  `-1` "no selection" sentinel, an offset left over from longer text, a reversed
  selection and an offset inside a surrogate pair are each clamped to a
  character boundary inside the text. Three of those panicked in release builds
  too (`move_up`, `move_right`, `move_left` on a freshly focused field).
- `TextEditingState::move_to` and `select_to` clamp their argument into the
  text, so an embedder driving the state through `with_state` cannot report a
  selection the framework considers invalid.
- `json_value!` no longer requires the calling crate to depend on `serde_json`
  and `log`, and logs a conversion failure instead of silently yielding
  `Value::Null`.

#### Memory safety and leaks

- `Reader::read_string` in the standard codec is bounds-checked; so is every
  other `Reader` primitive, and a length prefix is no longer trusted to size a
  `Vec`. A truncated or hostile message is a `DecodeError`, not an out-of-bounds
  read.
- The platform-message reply callback's user data is boxed and reclaimed at one
  type, which fixes a segfault on every `invoke_method_with_result` /
  `send_with_result` reply.
- `from_utf8_unchecked` is gone from the JSON and string codecs, and
  `slice::from_raw_parts` handles the engine's documented
  null-pointer-with-size-0 case.
- The engine-side response handle created for every
  `invoke_method_with_result` / `send_with_result` is released with
  `FlutterPlatformMessageReleaseResponseHandle` once the message has been sent,
  and the channel name's `CString` is no longer leaked on every outgoing
  message.
- `FlutterEngineInner` is no longer leaked: the callback user-data pointers are
  `Weak`, so the engine is actually dropped when the last handle goes, which is
  also what makes `FlutterEngineWeakRef::is_valid` meaningful — it was
  hard-wired to `true`.
- A posted-but-unconsumed texture frame's destruction callback runs when the
  `Texture` is dropped, instead of leaking the GL texture.
- The engine's argv, assets path and ICU data path are owned by the engine
  rather than `CString::into_raw`'d, so nothing leaks on the error path either.

#### Protocol correctness

- `codec::Value::F32List` decodes Dart's `Float32List` (wire tag 14). It
  previously decoded as `Value::Null` with the payload left unread, silently
  corrupting every later value in the same message.
- `LocalizationPlugin::send_locales` feeds `FlutterEngine::update_locales`
  instead of invoking `setLocale` on `flutter/localization`, a call the
  framework does not implement. The system locales therefore reach
  `PlatformDispatcher.locales` for the first time; before this, every app
  resolved to `supportedLocales.first` regardless of the system setting. It can
  be called from any thread and hops to the platform thread itself.
- `TextEditingState`'s offsets are UTF-16 code units, so selections, caret
  positions and the values reported back through
  `TextInputClient.updateEditingState` are now correct for text containing
  emoji or any other non-BMP character. (`delete_selected` on "👍abc" with
  Dart's 2..5 left "👍a" instead of "👍".)
- `FlutterEngine::send_pointer_event` timestamps events on the engine's
  monotonic `FlutterEngineGetCurrentTime` clock instead of the wall clock,
  which is what the embedder API requires, and the `scale` field of an ordinary
  pointer event is now 1.0 ("the initial scale" per the embedder header)
  instead of 0.0.
- flutter-glfw reports the cumulative mask of buttons actually held, so hovering
  no longer claims the primary button is down, two buttons at once are
  expressible, and a right- or middle-button drag reaches Dart as a drag rather
  than a hover. Mouse button 4 ("back") pops the navigation route and no longer
  emits an unbalanced pointer-up.
- flutter-glfw's device pixel ratio starts at 1.0 and falls back to 1.0 for a
  non-positive window or framebuffer size, so a single zero-size report can no
  longer poison it with NaN.
- flutter-glfw's GL proc resolver takes `*const c_char` rather than
  `*const i8`, which is a hard compile error on aarch64 where `c_char = u8`.
- Three GLFW key codes the SDK's own map omits (281 scroll lock, 313 F24, 333
  numpad minus) produce key events.
- The `flutter/keyboard` plugin answers `getKeyboardState` with null ("no
  information", which `HardwareKeyboard.syncKeyboardState` handles) instead of
  an empty `String`-keyed map that could never express the `Map<int, int>` the
  framework casts to.
- `Clipboard.getData` passes the mime error's text to Dart instead of
  discarding it.
- The `flutter-rs/dialog` plugin runs its native dialogs on a worker thread, so
  `open_file_dialog` and `message_box_ok` no longer block the engine's platform
  thread — the window keeps painting and answering the window manager while a
  dialog is open. Two concurrent calls queue instead of racing two dialogs, and
  a title, path, filter or message containing a NUL byte is reported as
  `invalid-args` rather than panicking inside `tinyfiledialogs`.
- The value deserializer hands strings and `Uint8List`s to the visitor as
  borrows of the `Value` (`visit_borrowed_str` / `visit_borrowed_bytes`), so
  `from_value::<&str>` works and a byte list is one memcpy instead of a
  per-element `visit_seq`.

#### SDK and engine discovery

- `Flutter::engine_version` prefers `bin/cache/engine.stamp` (what the `flutter`
  tool itself reads, present on every channel) and falls back to
  `bin/internal/engine.version`, so SDKs on main/master or a release-candidate
  branch no longer fail with a `NotFound` I/O error.
- `Engine::download` unpacks the Apple `FlutterEmbedder.framework.zip` into
  `engine_dir()/FlutterEmbedder.framework`, so `Engine::library_path` finally
  points at something that exists on Apple targets. Symlink entries in the
  archive are recreated as symlinks (rejecting absolute and escaping targets)
  rather than dropped.
- flutter-engine-sys looks the Android sysroot up under
  `toolchains/llvm/prebuilt/<host>/sysroot` (NDK r19 and newer), with a fallback
  to the legacy `$NDK/sysroot`, and accepts `ANDROID_NDK_HOME` /
  `ANDROID_NDK_ROOT` alongside `NDK_HOME`.
- Cross-compiling to an Apple target from a non-Apple host no longer aborts the
  build when `xcrun` is absent; `SDKROOT` is honoured and a warning is emitted.
- flutter-engine-sys's build script declares its inputs
  (`rerun-if-changed` / `rerun-if-env-changed`), so editing the vendored header
  actually regenerates the bindings and editing a source file does not.
- `scripts/build-engine.sh` reads the framework version from
  `bin/cache/flutter.version.json` (falling back to the git tag) and the engine
  revision from `bin/cache/engine.stamp`. It read `$FLUTTER_ROOT/version`, which
  no current SDK ships, so the banner always said "unknown" and the tag-based
  fetch fallback was dead code. Its `--help` no longer prints two lines of
  shell past the end of the header, and it tells you to pass `icudtl.dat` to
  `FlutterEngineBuilder::with_icu_data_path` rather than to a
  `FLUTTER_ICU_DATA` variable nothing reads.

### Removed

- `cargo-flutter` is no longer part of the documented workflow: it has not been
  released since 0.1.12 (2020-01-19) and no longer compiles. With it go the
  documented hot-reload (`cargo flutter run`) and distribution packaging
  (`cargo flutter --format appimage build --release`) paths, which are
  **currently unsupported**. Use `flutter-tools` or the download recipe in the
  README for a JIT engine, and `scripts/build-engine.sh` for an AOT one.
- `FlutterEngine::run_in_background` (it was `#[deprecated]` with no callers),
  and with it flutter-engine's `async-std` and `futures-task` dependencies
  (RUSTSEC-2025-0052: `async-std` is unmaintained).
- `flutter_engine::channel::event_channel`, the dead `flutter-glfw` `draw`
  module, and `flutter-plugins`' dead `StringUtils::substring`.
- flutter-glfw's `locale_config` dependency (unreleased since 2019, replaced by
  `sys-locale`), flutter-plugins' unused `serde_json` dependency, and
  flutter-glfw's unused direct dependency on flutter-engine-sys.

### Security

- `async-std` (RUSTSEC-2025-0052, unmaintained) is removed rather than
  ignored.
- `zip` is on 9.x, which addresses RUSTSEC-2026-0367; `flutter-tools`'
  extraction also masks the archive's unix mode with `& 0o777` and rejects
  symlink targets that are absolute or escape the destination.
- `cargo deny check advisories licenses bans sources` passes against the
  committed `Cargo.lock` and runs in CI. `priority-queue` is
  `LGPL-3.0-or-later OR MPL-2.0`; `deny.toml` records the MPL-2.0 choice, which
  keeps the workspace MIT-licensable.

### Build, CI and documentation

- The vendored `flutter-engine-sys/flutter-engine.h` is re-vendored from
  **Flutter 3.47.8** and is byte-identical to that release's
  `flutter_embedder.h`. The delta from the 3.44 header is purely additive
  (`kFlutterPointerDeviceKindInvertedStylus` and `FlutterPointerStylusButtons`),
  so the ABI is unchanged across 3.44 – 3.47.x.
- `.github/workflows/ci.yml` is rewritten. Every action it used was on a
  retired major, it installed `cargo-flutter`, it built `-p flutter-winit`
  (which is no longer a workspace member), and it carried Android and iOS
  matrix legs that cannot work. It now runs: build + clippy on Linux and macOS,
  tests on Linux against the engine library it downloads for the SDK's own
  engine revision, `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo doc` with `RUSTDOCFLAGS=-D warnings`, both MSRV floors, and
  `cargo deny`. Everything is `--locked`.
- `README.md` documents what works today instead of `cargo install
  cargo-flutter`, states the Flutter and Rust version ranges, and has no dead
  links. The Discord badge is removed: the invite had expired.
- `CONTRIBUTING.md` describes the five-crate workspace (it still described a
  three-crate one built on flutter-winit), lists the Linux system packages and
  the checks CI enforces, and records why `cargo miri test` is not run.
- `flutter-engine` and `flutter-glfw` have their own `README.md`; the other
  three crates point `readme` at the workspace one.
- `.gitignore` covers the engine output directories, so a 40 MB
  `libflutter_engine.so` cannot be staged by an unguarded `git add`.
- 109 unit tests, up from 9, all pure logic that needs no live engine: codecs
  (with byte fixtures generated from the real Dart SDK), the UTF-16 text-editing
  model, the pointer-button mask, task priority, the texture registry, error
  display, and the engine callbacks' behaviour when their user data is gone.

## 0.4.0 — unreleased

Never published to crates.io (the newest release there is 0.3.5). Listed after
0.3.0 in earlier copies of this file, which put the whole changelog out of
order.

- use winit instead of glfw
- use custom flutter engine builds
- new cli tool `cargo-flutter`
- refactor engine to be window framework agnostic
- dropped tokio and libc dependencies
- move plugins to `flutter-plugins`

## 0.3.0 — 2019-05-18

- Code refactor thanks to Sophie Tauchert.
- Multi channel plugin support.
- List scrolling.
- Update flutter embedder.
- Add flutter-engine-sys crate to generate bindings using bindgen.
- flutter-download now download flutter engine of the same version as your flutter sdk.
- Support linux snap distribution.

## 0.2.0 — 2019-03-22

- support windows
- nsis installer for windows
- new example UI

## 0.1.2 — 2019-01-22

The earliest tagged release; 0.1.0 and 0.1.1 preceded it on crates.io. See the
git history for details.
