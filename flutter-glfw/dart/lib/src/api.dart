// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

/// The C ABI between Flutter and the flutter-glfw Rust windowing backend.
///
/// `FlutterProjectArgs.engine_id` carries the address of the `FgWindowingApi`
/// table; `PlatformDispatcher.engineId` exposes it to Dart. The table's layout
/// must stay in sync with `flutter-glfw/src/windowing.rs`.
library;

import 'dart:ffi';
import 'dart:ui' show PlatformDispatcher;

import 'package:ffi/ffi.dart';

/// `FgWindowParams::flags` bits.
const int fgFlagHasSize = 1 << 0;
const int fgFlagResizable = 1 << 1;
const int fgFlagDecorated = 1 << 2;
const int fgFlagShrinkWrap = 1 << 3;
const int fgFlagHasPosition = 1 << 4;
const int fgFlagFloating = 1 << 5;
const int fgFlagModal = 1 << 6;

/// Window kinds, matching `KIND_*` in Rust.
const int fgKindRegular = 0;
const int fgKindDialog = 1;
const int fgKindTooltip = 2;
const int fgKindPopup = 3;
const int fgKindSatellite = 4;

/// Event codes, matching `flutter_glfw::app::event`.
const int fgEventCloseRequested = 1;
const int fgEventDestroyed = 2;
const int fgEventSizeChanged = 3;
const int fgEventPositionChanged = 4;
const int fgEventScaleChanged = 5;
const int fgEventFocusChanged = 6;
const int fgEventMinimizedChanged = 7;
const int fgEventMaximizedChanged = 8;
const int fgEventFullscreenChanged = 9;

/// `FgWindowParams` in `windowing.rs`.
final class FgWindowParams extends Struct {
  @Int32()
  external int kind;

  @Uint32()
  external int flags;

  @Double()
  external double width;

  @Double()
  external double height;

  @Int64()
  external int parent;

  @Double()
  external double x;

  @Double()
  external double y;

  external Pointer<Utf8> title;
}

typedef _CreateWindowC = Int64 Function(Pointer<FgWindowParams>);
typedef _CreateWindowD = int Function(Pointer<FgWindowParams>);
typedef _WindowIdC = Int32 Function(Int64);
typedef _WindowIdD = int Function(int);
typedef _SetTitleC = Int32 Function(Int64, Pointer<Char>);
typedef _SetTitleD = int Function(int, Pointer<Char>);
typedef _GetSizeC = Int32 Function(Int64, Pointer<Double>, Pointer<Double>);
typedef _GetSizeD = int Function(int, Pointer<Double>, Pointer<Double>);
typedef _SetSizeC = Int32 Function(Int64, Double, Double);
typedef _SetSizeD = int Function(int, double, double);
typedef _SetConstraintsC = Int32 Function(
  Int64,
  Double,
  Double,
  Double,
  Double,
);
typedef _SetConstraintsD = int Function(int, double, double, double, double);
typedef _GetPositionC = Int32 Function(Int64, Pointer<Double>, Pointer<Double>);
typedef _GetPositionD = int Function(int, Pointer<Double>, Pointer<Double>);
typedef _SetPositionC = Int32 Function(Int64, Double, Double);
typedef _SetPositionD = int Function(int, double, double);
typedef _SetBoolC = Int32 Function(Int64, Int32);
typedef _SetBoolD = int Function(int, int);
typedef _GetBoolC = Int32 Function(Int64);
typedef _GetBoolD = int Function(int);
typedef _GetScaleC = Double Function(Int64);
typedef _GetScaleD = double Function(int);
typedef _WorkAreaC = Int32 Function(
  Pointer<Double>,
  Pointer<Double>,
  Pointer<Double>,
  Pointer<Double>,
);
typedef _WorkAreaD = int Function(
  Pointer<Double>,
  Pointer<Double>,
  Pointer<Double>,
  Pointer<Double>,
);
typedef _SetEventCallbackC = Int32 Function(Pointer<Void>, Pointer<Void>);
typedef _SetEventCallbackD = int Function(Pointer<Void>, Pointer<Void>);
typedef _SetParentC = Int32 Function(Int64, Int64);
typedef _SetParentD = int Function(int, int);
typedef _UsesLogicalC = Int32 Function();
typedef _UsesLogicalD = int Function();

/// The function table in `windowing.rs`. Field order and types must match
/// `FgWindowingApi` exactly.
final class FgWindowingApi extends Struct {
  @Uint32()
  external int version;

  @Uint32()
  external int size;

  external Pointer<NativeFunction<_CreateWindowC>> createWindow;
  external Pointer<NativeFunction<_WindowIdC>> destroyWindow;
  external Pointer<NativeFunction<_SetTitleC>> setTitle;
  external Pointer<NativeFunction<_GetSizeC>> getSize;
  external Pointer<NativeFunction<_SetSizeC>> setSize;
  external Pointer<NativeFunction<_SetConstraintsC>> setConstraints;
  external Pointer<NativeFunction<_GetPositionC>> getPosition;
  external Pointer<NativeFunction<_SetPositionC>> setPosition;
  external Pointer<NativeFunction<_SetBoolC>> setMaximized;
  external Pointer<NativeFunction<_GetBoolC>> isMaximized;
  external Pointer<NativeFunction<_SetBoolC>> setMinimized;
  external Pointer<NativeFunction<_GetBoolC>> isMinimized;
  external Pointer<NativeFunction<_SetBoolC>> setFullscreen;
  external Pointer<NativeFunction<_GetBoolC>> isFullscreen;
  external Pointer<NativeFunction<_WindowIdC>> activate;
  external Pointer<NativeFunction<_GetBoolC>> isActivated;
  external Pointer<NativeFunction<_GetScaleC>> getScale;
  external Pointer<NativeFunction<_WorkAreaC>> getMonitorWorkarea;
  external Pointer<NativeFunction<_SetEventCallbackC>> setEventCallback;
  external Pointer<NativeFunction<_SetParentC>> setParent;
  external Pointer<NativeFunction<_UsesLogicalC>> usesLogicalScreenCoordinates;
  external Pointer<NativeFunction<_GetPositionC>> getPositionPhysical;
  external Pointer<NativeFunction<_GetSizeC>> getSizePhysical;
  external Pointer<NativeFunction<_WorkAreaC>> getMonitorWorkareaPhysical;
  external Pointer<NativeFunction<_SetPositionC>> setPositionPhysical;
}

/// The Dart-facing wrapper around the native windowing table.
class GlfwWindowingApi {
  GlfwWindowingApi._(FgWindowingApi api)
    : _createWindow = api.createWindow.asFunction(),
      _destroyWindow = api.destroyWindow.asFunction(),
      _setTitle = api.setTitle.asFunction(),
      _getSize = api.getSize.asFunction(),
      _setSize = api.setSize.asFunction(),
      _setConstraints = api.setConstraints.asFunction(),
      _getPosition = api.getPosition.asFunction(),
      _setPosition = api.setPosition.asFunction(),
      _setMaximized = api.setMaximized.asFunction(),
      _isMaximized = api.isMaximized.asFunction(),
      _setMinimized = api.setMinimized.asFunction(),
      _isMinimized = api.isMinimized.asFunction(),
      _setFullscreen = api.setFullscreen.asFunction(),
      _isFullscreen = api.isFullscreen.asFunction(),
      _activate = api.activate.asFunction(),
      _isActivated = api.isActivated.asFunction(),
      _getScale = api.getScale.asFunction(),
      _getMonitorWorkarea = api.getMonitorWorkarea.asFunction(),
      _setEventCallback = api.setEventCallback.asFunction(),
      _setParent = api.setParent.asFunction(),
      _usesLogicalScreenCoordinates =
          api.usesLogicalScreenCoordinates.asFunction(),
      _getPositionPhysical = api.getPositionPhysical.asFunction(),
      _getSizePhysical = api.getSizePhysical.asFunction(),
      _getMonitorWorkareaPhysical = api.getMonitorWorkareaPhysical.asFunction(),
      _setPositionPhysical = api.setPositionPhysical.asFunction();

  /// Resolve the table from `PlatformDispatcher.engineId`.
  ///
  /// Throws when the engine did not install the flutter-glfw windowing ABI.
  factory GlfwWindowingApi.fromEngine() {
    final int? engineId = PlatformDispatcher.instance.engineId;
    if (engineId == null || engineId == 0) {
      throw StateError(
        'PlatformDispatcher.engineId is null: the Flutter engine was not '
        'started by flutter-glfw, or the windowing ABI was not installed.',
      );
    }
    final FgWindowingApi api = Pointer<FgWindowingApi>.fromAddress(engineId)
        .ref;
    if (api.version != 1 || api.size != sizeOf<FgWindowingApi>()) {
      throw StateError(
        'flutter-glfw windowing ABI mismatch: version ${api.version}, '
        'size ${api.size}, expected ${sizeOf<FgWindowingApi>()}. '
        'Keep flutter_glfw_windowing and flutter-glfw in step.',
      );
    }
    return GlfwWindowingApi._(api);
  }

  final _CreateWindowD _createWindow;
  final _WindowIdD _destroyWindow;
  final _SetTitleD _setTitle;
  final _GetSizeD _getSize;
  final _SetSizeD _setSize;
  final _SetConstraintsD _setConstraints;
  final _GetPositionD _getPosition;
  final _SetPositionD _setPosition;
  final _SetBoolD _setMaximized;
  final _GetBoolD _isMaximized;
  final _SetBoolD _setMinimized;
  final _GetBoolD _isMinimized;
  final _SetBoolD _setFullscreen;
  final _GetBoolD _isFullscreen;
  final _WindowIdD _activate;
  final _GetBoolD _isActivated;
  final _GetScaleD _getScale;
  final _WorkAreaD _getMonitorWorkarea;
  final _SetEventCallbackD _setEventCallback;
  final _SetParentD _setParent;
  final _UsesLogicalD _usesLogicalScreenCoordinates;
  final _GetPositionD _getPositionPhysical;
  final _GetSizeD _getSizePhysical;
  final _WorkAreaD _getMonitorWorkareaPhysical;
  final _SetPositionD _setPositionPhysical;

  /// Create a window; returns the engine view id, or `-1`.
  int createWindow({
    required int kind,
    required int flags,
    required double width,
    required double height,
    int parent = -1,
    double x = 0,
    double y = 0,
    required String title,
  }) {
    final Pointer<FgWindowParams> params = calloc<FgWindowParams>();
    final Pointer<Utf8> titlePtr = title.toNativeUtf8();
    try {
      params.ref
        ..kind = kind
        ..flags = flags
        ..width = width
        ..height = height
        ..parent = parent
        ..x = x
        ..y = y
        ..title = titlePtr;
      return _createWindow(params);
    } finally {
      calloc.free(params);
      calloc.free(titlePtr);
    }
  }

  /// Destroy a window and remove its view.
  bool destroyWindow(int viewId) => _destroyWindow(viewId) == 0;

  /// Set the window title.
  bool setTitle(int viewId, String title) {
    final Pointer<Utf8> titlePtr = title.toNativeUtf8();
    try {
      return _setTitle(viewId, titlePtr.cast<Char>()) == 0;
    } finally {
      calloc.free(titlePtr);
    }
  }

  /// The logical content size, or `Size.zero` on failure.
  ({double width, double height}) getSize(int viewId) {
    final Pointer<Double> out = calloc<Double>(2);
    try {
      if (_getSize(viewId, out, out + 1) != 0) {
        return (width: 0, height: 0);
      }
      return (width: out[0], height: out[1]);
    } finally {
      calloc.free(out);
    }
  }

  /// Set the logical content size.
  bool setSize(int viewId, double width, double height) =>
      _setSize(viewId, width, height) == 0;

  /// Set size limits; a non-positive bound means unbounded.
  bool setConstraints(
    int viewId,
    double minWidth,
    double minHeight,
    double maxWidth,
    double maxHeight,
  ) => _setConstraints(viewId, minWidth, minHeight, maxWidth, maxHeight) == 0;

  /// The logical window position.
  ({double x, double y}) getPosition(int viewId) {
    final Pointer<Double> out = calloc<Double>(2);
    try {
      if (_getPosition(viewId, out, out + 1) != 0) {
        return (x: 0, y: 0);
      }
      return (x: out[0], y: out[1]);
    } finally {
      calloc.free(out);
    }
  }

  /// Set the logical window position.
  bool setPosition(int viewId, double x, double y) =>
      _setPosition(viewId, x, y) == 0;

  bool setMaximized(int viewId, bool maximized) =>
      _setMaximized(viewId, maximized ? 1 : 0) == 0;
  bool isMaximized(int viewId) => _isMaximized(viewId) != 0;
  bool setMinimized(int viewId, bool minimized) =>
      _setMinimized(viewId, minimized ? 1 : 0) == 0;
  bool isMinimized(int viewId) => _isMinimized(viewId) != 0;
  bool setFullscreen(int viewId, bool fullscreen) =>
      _setFullscreen(viewId, fullscreen ? 1 : 0) == 0;
  bool isFullscreen(int viewId) => _isFullscreen(viewId) != 0;
  bool activate(int viewId) => _activate(viewId) == 0;
  bool isActivated(int viewId) => _isActivated(viewId) != 0;
  double getScale(int viewId) => _getScale(viewId);
  bool setParent(int viewId, int parent) => _setParent(viewId, parent) == 0;

  /// Whether GLFW's screen coordinates are logical (macOS points, Wayland).
  bool usesLogicalScreenCoordinates() => _usesLogicalScreenCoordinates() != 0;

  /// The raw GLFW position (physical on X11/Windows).
  ({double x, double y}) getPositionPhysical(int viewId) {
    final Pointer<Double> out = calloc<Double>(2);
    try {
      if (_getPositionPhysical(viewId, out, out + 1) != 0) {
        return (x: 0, y: 0);
      }
      return (x: out[0], y: out[1]);
    } finally {
      calloc.free(out);
    }
  }

  /// The raw framebuffer size in pixels.
  ({double width, double height}) getSizePhysical(int viewId) {
    final Pointer<Double> out = calloc<Double>(2);
    try {
      if (_getSizePhysical(viewId, out, out + 1) != 0) {
        return (width: 0, height: 0);
      }
      return (width: out[0], height: out[1]);
    } finally {
      calloc.free(out);
    }
  }

  /// The primary monitor's work area in physical pixels.
  ({double x, double y, double width, double height})
  getMonitorWorkareaPhysical() {
    final Pointer<Double> out = calloc<Double>(4);
    try {
      if (_getMonitorWorkareaPhysical(out, out + 1, out + 2, out + 3) != 0) {
        return (x: 0, y: 0, width: 1920, height: 1080);
      }
      return (x: out[0], y: out[1], width: out[2], height: out[3]);
    } finally {
      calloc.free(out);
    }
  }

  /// Set the raw GLFW position (physical on X11/Windows).
  bool setPositionPhysical(int viewId, double x, double y) =>
      _setPositionPhysical(viewId, x, y) == 0;

  /// The primary monitor's work area in logical screen coordinates.
  ({double x, double y, double width, double height}) getMonitorWorkarea() {
    final Pointer<Double> out = calloc<Double>(4);
    try {
      if (_getMonitorWorkarea(out, out + 1, out + 2, out + 3) != 0) {
        return (x: 0, y: 0, width: 1920, height: 1080);
      }
      return (x: out[0], y: out[1], width: out[2], height: out[3]);
    } finally {
      calloc.free(out);
    }
  }

  /// Install the native event callback. The callable must stay alive.
  bool setEventCallback(Pointer<Void> callback) =>
      _setEventCallback(callback, nullptr) == 0;
}
