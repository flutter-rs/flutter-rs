// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

/// A [WindowingOwner] backed by the flutter-glfw Rust embedder.
library;

// ignore_for_file: implementation_imports, invalid_use_of_internal_member

import 'dart:ffi' hide Size;
import 'dart:ui' show Display;

import 'package:flutter/foundation.dart';
import 'package:flutter/rendering.dart' show BoxConstraints;
import 'package:flutter/src/foundation/_features.dart' show isWindowingEnabled;
import 'package:flutter/src/widgets/_window.dart';
import 'package:flutter/src/widgets/_window_positioner.dart';
import 'package:flutter/widgets.dart'
    show Offset, Rect, Size, WidgetsBinding, WidgetsFlutterBinding;

import 'api.dart';

typedef _EventCallbackC = Void Function(
  Int64 viewId,
  Int32 event,
  Double arg0,
  Double arg1,
  Pointer<Void> data,
);

const double _defaultWindowWidth = 400;
const double _defaultWindowHeight = 300;
const double _defaultPopupWidth = 240;
const double _defaultPopupHeight = 120;

/// A [WidgetsFlutterBinding] that installs a [GlfwWindowingOwner] while it
/// initializes.
///
/// Use this instead of `WidgetsFlutterBinding.ensureInitialized()` in an app
/// started by flutter-glfw:
///
/// ```dart
/// void main() {
///   GlfwWindowingBinding();
///   runApp(const App());
/// }
/// ```
///
/// Assigning `WidgetsBinding.instance.windowingOwner` after the default
/// binding has initialized is too late on Windows. The default binding asks
/// `createWindowingOwner` for the platform owner before user code runs, and on
/// Windows that is a `WindowingOwnerWin32`, which needs native symbols only
/// Flutter's own Windows embedder exports. The generic embedder flutter-glfw
/// loads does not, so the binding throws before the assignment can happen.
class GlfwWindowingBinding extends WidgetsFlutterBinding {
  @override
  WindowingOwner createWindowingOwner() {
    isWindowingEnabled = true;

    return GlfwWindowingOwner();
  }
}

/// The GLFW windowing owner.
///
/// [GlfwWindowingBinding] installs it, which is the supported way to enable
/// windowing in an app started by flutter-glfw. Assigning it to
/// `WidgetsBinding.instance.windowingOwner` by hand only works on platforms
/// whose default owner has no native dependency (Linux and macOS).
///
/// The owner reads the Rust function table from
/// `PlatformDispatcher.engineId`, so it only works in an app started by
/// flutter-glfw.
class GlfwWindowingOwner extends WindowingOwner {
  GlfwWindowingOwner({GlfwWindowingApi? api})
    : _api = api ?? GlfwWindowingApi.fromEngine() {
    _eventCallable = NativeCallable<_EventCallbackC>.listener(_onEvent);
    _api.setEventCallback(_eventCallable.nativeFunction.cast<Void>());
  }

  final GlfwWindowingApi _api;
  late final NativeCallable<_EventCallbackC> _eventCallable;
  final Map<int, _GlfwController> _controllers = <int, _GlfwController>{};

  /// The native API, exposed for tests and advanced callers.
  GlfwWindowingApi get api => _api;

  void _onEvent(
    int viewId,
    int event,
    double arg0,
    double arg1,
    Pointer<Void> data,
  ) {
    final _GlfwController? controller = _controllers[viewId];
    if (controller == null) {
      return;
    }
    switch (event) {
      case fgEventCloseRequested:
        controller.handleCloseRequested();
      case fgEventDestroyed:
        controller.handleDestroyedEvent();
      case fgEventSizeChanged:
      case fgEventPositionChanged:
      case fgEventScaleChanged:
      case fgEventFocusChanged:
      case fgEventMinimizedChanged:
      case fgEventMaximizedChanged:
      case fgEventFullscreenChanged:
        controller.notifyGeometryChanged();
    }
  }

  void unregister(int viewId) {
    _controllers.remove(viewId);
  }

  int _create({
    required int kind,
    required Size size,
    required bool hasSize,
    required bool resizable,
    required bool decorated,
    required bool shrinkWrap,
    required bool floating,
    required bool modal,
    required int parent,
    Offset? position,
    required String? title,
  }) {
    int flags = 0;
    if (hasSize) {
      flags |= fgFlagHasSize;
    }
    if (resizable) {
      flags |= fgFlagResizable;
    }
    if (decorated) {
      flags |= fgFlagDecorated;
    }
    if (shrinkWrap) {
      flags |= fgFlagShrinkWrap;
    }
    if (floating) {
      flags |= fgFlagFloating;
    }
    if (modal) {
      flags |= fgFlagModal;
    }
    if (position != null) {
      flags |= fgFlagHasPosition;
    }
    final int viewId = _api.createWindow(
      kind: kind,
      flags: flags,
      width: size.width,
      height: size.height,
      parent: parent,
      x: position?.dx ?? 0,
      y: position?.dy ?? 0,
      title: title ?? '',
    );
    if (viewId < 0) {
      throw StateError('flutter-glfw could not create a window (kind $kind)');
    }
    if (WidgetsBinding.instance.platformDispatcher.view(id: viewId) == null) {
      throw StateError(
        'flutter-glfw created view $viewId but it is not on the '
        'PlatformDispatcher; is the UI isolate running on the platform thread?',
      );
    }
    return viewId;
  }

  Size _sizeFrom(
    Size? size,
    BoxConstraints? constraints,
    double width,
    double height,
  ) {
    if (size != null) {
      return size;
    }
    if (constraints != null) {
      final double w = constraints.hasBoundedWidth
          ? constraints.maxWidth
          : width;
      final double h = constraints.hasBoundedHeight
          ? constraints.maxHeight
          : height;
      if (w > 0 && h > 0) {
        return Size(w, h);
      }
    }
    return Size(width, height);
  }

  Rect _parentRect(BaseWindowController? parent) {
    final _GlfwController? controller = parent is _GlfwController
        ? parent as _GlfwController
        : null;
    if (controller != null) {
      final ({double x, double y}) pos = _api.getPosition(controller.viewId);
      final Size size = controller.contentSize;
      return Rect.fromLTWH(pos.x, pos.y, size.width, size.height);
    }
    if (parent != null) {
      return Offset.zero & parent.contentSize;
    }
    return Offset.zero & const Size(_defaultWindowWidth, _defaultWindowHeight);
  }

  Rect _displayRect() {
    final ({double x, double y, double width, double height}) area = _api
        .getMonitorWorkarea();
    return Rect.fromLTWH(area.x, area.y, area.width, area.height);
  }

  /// Converts an anchor rectangle from the parent's content coordinates to
  /// global screen coordinates.
  ///
  /// The framework hands tooltip, popup and satellite controllers an
  /// `anchorRect` in the parent's coordinate space, but
  /// [WindowPositioner.placeWindow] expects every rectangle in the same global
  /// space as [parentRect]. Passing a local rectangle straight through makes
  /// the positioner clamp its anchor point to the parent's top-left edge, so
  /// the child is placed at the parent's origin instead of at the anchor.
  Rect _globalAnchorRect(BaseWindowController parent, Rect anchorRect) {
    return anchorRect.shift(_parentRect(parent).topLeft);
  }

  Offset _place({
    required Size childSize,
    required Rect anchorRect,
    required BaseWindowController parent,
    required WindowPositioner positioner,
  }) {
    return positioner
        .placeWindow(
          childSize: childSize,
          // `anchorRect` is already in global coordinates: callers either pass
          // a parent rect or a local rect through [_globalAnchorRect].
          anchorRect: anchorRect,
          parentRect: _parentRect(parent),
          displayRect: _displayRect(),
        )
        .topLeft;
  }

  @override
  WindowController createWindowController({
    required WindowControllerDelegate delegate,
    Size? size,
    BoxConstraints? constraints,
    required bool resizable,
    String? title,
  }) {
    final bool shrinkWrap = size == null;
    final Size effective = _sizeFrom(
      size,
      constraints,
      _defaultWindowWidth,
      _defaultWindowHeight,
    );
    final int viewId = _create(
      kind: fgKindRegular,
      size: effective,
      hasSize: !shrinkWrap,
      resizable: resizable,
      decorated: true,
      shrinkWrap: shrinkWrap,
      floating: false,
      modal: false,
      parent: -1,
      title: title,
    );
    final controller = _GlfwWindowController(
      owner: this,
      api: _api,
      viewId: viewId,
      delegate: delegate,
      title: title,
      constraints: constraints,
    );
    _controllers[viewId] = controller;
    return controller;
  }

  @override
  DialogWindowController createDialogWindowController({
    required DialogWindowControllerDelegate delegate,
    Size? size,
    BoxConstraints? constraints,
    required bool resizable,
    BaseWindowController? parent,
    String? title,
  }) {
    final bool shrinkWrap = size == null;
    final Size effective = _sizeFrom(
      size,
      constraints,
      _defaultWindowWidth,
      _defaultWindowHeight,
    );
    final Offset? position = parent == null
        ? null
        : _place(
            childSize: effective,
            anchorRect: _parentRect(parent),
            parent: parent,
            positioner: const WindowPositioner(),
          );
    final int viewId = _create(
      kind: fgKindDialog,
      size: effective,
      hasSize: !shrinkWrap,
      resizable: resizable,
      decorated: true,
      shrinkWrap: shrinkWrap,
      floating: false,
      modal: parent != null,
      parent: parent?.rootView.viewId ?? -1,
      position: position,
      title: title,
    );
    final controller = _GlfwDialogWindowController(
      owner: this,
      api: _api,
      viewId: viewId,
      delegate: delegate,
      parent: parent,
      title: title,
      constraints: constraints,
    );
    _controllers[viewId] = controller;
    return controller;
  }

  @override
  TooltipWindowController createTooltipWindowController({
    required TooltipWindowControllerDelegate delegate,
    required BoxConstraints constraints,
    required Rect anchorRect,
    required WindowPositioner positioner,
    required BaseWindowController parent,
  }) {
    final Size childSize = _sizeFrom(
      null,
      constraints,
      _defaultPopupWidth,
      _defaultPopupHeight,
    );
    final Offset position = _place(
      childSize: childSize,
      anchorRect: _globalAnchorRect(parent, anchorRect),
      parent: parent,
      positioner: positioner,
    );
    final int viewId = _create(
      kind: fgKindTooltip,
      size: childSize,
      hasSize: false,
      resizable: false,
      decorated: false,
      shrinkWrap: true,
      floating: true,
      modal: false,
      parent: parent.rootView.viewId,
      position: position,
      title: null,
    );
    final controller = _GlfwTooltipWindowController(
      owner: this,
      api: _api,
      viewId: viewId,
      delegate: delegate,
      parent: parent,
      anchorRect: anchorRect,
      positioner: positioner,
    );
    _controllers[viewId] = controller;
    return controller;
  }

  @override
  PopupWindowController createPopupWindowController({
    required PopupWindowControllerDelegate delegate,
    required BoxConstraints constraints,
    required Rect anchorRect,
    required WindowPositioner positioner,
    required BaseWindowController parent,
  }) {
    final Size childSize = _sizeFrom(
      null,
      constraints,
      _defaultPopupWidth,
      _defaultPopupHeight,
    );
    final Offset position = _place(
      childSize: childSize,
      anchorRect: _globalAnchorRect(parent, anchorRect),
      parent: parent,
      positioner: positioner,
    );
    final int viewId = _create(
      kind: fgKindPopup,
      size: childSize,
      hasSize: false,
      resizable: false,
      decorated: false,
      shrinkWrap: true,
      floating: true,
      modal: false,
      parent: parent.rootView.viewId,
      position: position,
      title: null,
    );
    final controller = _GlfwPopupWindowController(
      owner: this,
      api: _api,
      viewId: viewId,
      delegate: delegate,
      parent: parent,
      anchorRect: anchorRect,
      positioner: positioner,
    );
    _controllers[viewId] = controller;
    return controller;
  }

  @override
  SatelliteWindowController createSatelliteWindowController({
    required SatelliteWindowControllerDelegate delegate,
    required BaseWindowController parent,
    required WindowPositioner initialPositioner,
    Rect? initialAnchorRect,
    Size? size,
    BoxConstraints? constraints,
    required bool resizable,
    String? title,
  }) {
    final bool shrinkWrap = size == null;
    final Size effective = _sizeFrom(
      size,
      constraints,
      _defaultWindowWidth,
      _defaultWindowHeight,
    );
    final int parentViewId = parent.rootView.viewId;
    final bool logicalCoordinates = _api.usesLogicalScreenCoordinates();
    final Rect logicalParentRect = _parentRect(parent);
    final Offset? position = logicalCoordinates
        ? initialPositioner
              .placeWindow(
                childSize: effective,
                anchorRect: initialAnchorRect == null
                    ? logicalParentRect
                    : initialAnchorRect.shift(logicalParentRect.topLeft),
                parentRect: logicalParentRect,
                displayRect: _displayRect(),
              )
              .topLeft
        : null;
    final int viewId = _create(
      kind: fgKindSatellite,
      size: effective,
      hasSize: !shrinkWrap,
      resizable: resizable,
      decorated: true,
      shrinkWrap: shrinkWrap,
      floating: true,
      modal: false,
      parent: parentViewId,
      position: position,
      title: title,
    );
    if (!logicalCoordinates) {
      // Mixed-DPI X11 sessions can report different content scales for the
      // parent and the freshly mapped child. Place the satellite from the raw
      // physical rectangles and its real framebuffer size instead of scaling
      // logical values, so the child ends up flush with the parent.
      final ({double x, double y}) parentPosition = _api.getPositionPhysical(
        parentViewId,
      );
      final ({double width, double height}) parentSize = _api.getSizePhysical(
        parentViewId,
      );
      final ({double width, double height}) childSize = _api.getSizePhysical(
        viewId,
      );
      final Size parentLogical = parent.contentSize;
      final double scale = parentLogical.width > 0
          ? parentSize.width / parentLogical.width
          : _api.getScale(parentViewId);
      final Rect parentRect = Rect.fromLTWH(
        parentPosition.x,
        parentPosition.y,
        parentSize.width,
        parentSize.height,
      );
      final Rect anchorRect = initialAnchorRect == null
          ? parentRect
          : Rect.fromLTWH(
              parentPosition.x + initialAnchorRect.left * scale,
              parentPosition.y + initialAnchorRect.top * scale,
              initialAnchorRect.width * scale,
              initialAnchorRect.height * scale,
            );
      final ({double x, double y, double width, double height}) display = _api
          .getMonitorWorkareaPhysical();
      final Rect placed = initialPositioner.placeWindow(
        childSize: Size(childSize.width, childSize.height),
        anchorRect: anchorRect,
        parentRect: parentRect,
        displayRect: Rect.fromLTWH(
          display.x,
          display.y,
          display.width,
          display.height,
        ),
      );
      _api.setPositionPhysical(viewId, placed.left, placed.top);
    }
    final controller = _GlfwSatelliteWindowController(
      owner: this,
      api: _api,
      viewId: viewId,
      delegate: delegate,
      parent: parent,
      title: title,
      constraints: constraints,
    );
    _controllers[viewId] = controller;
    return controller;
  }
}

/// What the owner needs from every controller.
abstract interface class _GlfwController {
  int get viewId;
  Size get contentSize;
  void handleCloseRequested();
  void handleDestroyedEvent();
  void notifyGeometryChanged();
}

/// Shared native-window behaviour.
mixin _GlfwCommon on ChangeNotifier implements _GlfwController {
  GlfwWindowingApi get api;

  @override
  int get viewId;

  GlfwWindowingOwner get owner;

  bool _destroyed = false;

  bool get isDestroyed => _destroyed;

  @override
  Size get contentSize {
    final ({double width, double height}) size = api.getSize(viewId);
    return Size(size.width, size.height);
  }

  @override
  void notifyGeometryChanged() {
    notifyListeners();
  }

  /// The last position requested for an anchored child window.
  Offset? _lastAnchoredPosition;

  /// Positions this window at [anchorRect], which is in the parent's content
  /// coordinates.
  ///
  /// Tooltip and popup windows shrink-wrap to their content, so their final
  /// size is only known after the first frame. Recomputing on every geometry
  /// event, and skipping an identical target, keeps the window centred on its
  /// anchor as the content settles without fighting the window manager's
  /// echo of our own set-position request.
  void placeAtAnchor({
    required BaseWindowController parent,
    required Rect anchorRect,
    required WindowPositioner positioner,
  }) {
    final Size size = contentSize;
    if (size.width <= 0 || size.height <= 0) {
      return;
    }
    final Rect parentRect = owner._parentRect(parent);
    final Offset position = positioner
        .placeWindow(
          childSize: size,
          anchorRect: anchorRect.shift(parentRect.topLeft),
          parentRect: parentRect,
          displayRect: owner._displayRect(),
        )
        .topLeft;
    if (position == _lastAnchoredPosition) {
      return;
    }
    _lastAnchoredPosition = position;
    api.setPosition(viewId, position.dx, position.dy);
  }

  @override
  void handleDestroyedEvent() {
    _finishDestroy();
  }

  void _finishDestroy() {
    if (_destroyed) {
      return;
    }
    _destroyed = true;
    owner.unregister(viewId);
    onNativeDestroyed();
    notifyListeners();
  }

  /// Called once when the native window is gone; invokes the right delegate.
  void onNativeDestroyed();

  void destroy() {
    if (_destroyed) {
      return;
    }
    api.destroyWindow(viewId);
    _finishDestroy();
  }

  void setConstraints(BoxConstraints constraints) {
    api.setConstraints(
      viewId,
      constraints.minWidth,
      constraints.minHeight,
      constraints.hasBoundedWidth ? constraints.maxWidth : 0,
      constraints.hasBoundedHeight ? constraints.maxHeight : 0,
    );
  }
}

class _GlfwWindowController extends WindowController with _GlfwCommon {
  _GlfwWindowController({
    required this.owner,
    required this.api,
    required this.viewId,
    required WindowControllerDelegate delegate,
    String? title,
    BoxConstraints? constraints,
  }) : _delegate = delegate,
       _title = title ?? '',
       _constraints = constraints,
       super.empty() {
    rootView = WidgetsBinding.instance.platformDispatcher.view(id: viewId)!;
    if (_constraints != null) {
      setConstraints(_constraints);
    }
  }

  @override
  final GlfwWindowingApi api;

  @override
  final int viewId;

  @override
  final GlfwWindowingOwner owner;

  final WindowControllerDelegate _delegate;
  final BoxConstraints? _constraints;
  String _title;

  @override
  String get title => _title;

  @override
  bool get isActivated => api.isActivated(viewId);

  @override
  bool get isMaximized => api.isMaximized(viewId);

  @override
  bool get isMinimized => api.isMinimized(viewId);

  @override
  bool get isFullscreen => api.isFullscreen(viewId);

  @override
  void setSize(Size size) {
    api.setSize(viewId, size.width, size.height);
  }

  @override
  void setTitle(String title) {
    _title = title;
    api.setTitle(viewId, title);
  }

  @override
  void activate() {
    api.activate(viewId);
  }

  @override
  void setMaximized(bool maximized) {
    api.setMaximized(viewId, maximized);
  }

  @override
  void setMinimized(bool minimized) {
    api.setMinimized(viewId, minimized);
  }

  @override
  void setFullscreen(bool fullscreen, {Display? display}) {
    api.setFullscreen(viewId, fullscreen);
  }

  @override
  void handleCloseRequested() {
    _delegate.onWindowCloseRequested(this);
  }

  @override
  void onNativeDestroyed() {
    _delegate.onWindowDestroyed();
  }

  @override
  void dispose() {
    owner.unregister(viewId);
    super.dispose();
  }
}

class _GlfwDialogWindowController extends DialogWindowController
    with _GlfwCommon {
  _GlfwDialogWindowController({
    required this.owner,
    required this.api,
    required this.viewId,
    required DialogWindowControllerDelegate delegate,
    required this.parent,
    String? title,
    BoxConstraints? constraints,
  }) : _delegate = delegate,
       _title = title ?? '',
       _constraints = constraints,
       super.empty() {
    rootView = WidgetsBinding.instance.platformDispatcher.view(id: viewId)!;
    if (_constraints != null) {
      setConstraints(_constraints);
    }
  }

  @override
  final GlfwWindowingApi api;

  @override
  final int viewId;

  @override
  final GlfwWindowingOwner owner;

  final DialogWindowControllerDelegate _delegate;
  final BoxConstraints? _constraints;
  String _title;

  @override
  final BaseWindowController? parent;

  @override
  String get title => _title;

  @override
  bool get isActivated => api.isActivated(viewId);

  @override
  bool get isMinimized => api.isMinimized(viewId);

  @override
  void setSize(Size size) {
    api.setSize(viewId, size.width, size.height);
  }

  @override
  void setTitle(String title) {
    _title = title;
    api.setTitle(viewId, title);
  }

  @override
  void activate() {
    api.activate(viewId);
  }

  @override
  void setMinimized(bool minimized) {
    api.setMinimized(viewId, minimized);
  }

  @override
  void handleCloseRequested() {
    _delegate.onWindowCloseRequested(this);
  }

  @override
  void onNativeDestroyed() {
    _delegate.onWindowDestroyed();
  }

  @override
  void dispose() {
    owner.unregister(viewId);
    super.dispose();
  }
}

class _GlfwTooltipWindowController extends TooltipWindowController
    with _GlfwCommon {
  _GlfwTooltipWindowController({
    required this.owner,
    required this.api,
    required this.viewId,
    required TooltipWindowControllerDelegate delegate,
    required this.parent,
    required Rect anchorRect,
    required WindowPositioner positioner,
  }) : _delegate = delegate,
       _anchorRect = anchorRect,
       _positioner = positioner,
       super.empty() {
    rootView = WidgetsBinding.instance.platformDispatcher.view(id: viewId)!;
  }

  @override
  final GlfwWindowingApi api;

  @override
  final int viewId;

  @override
  final GlfwWindowingOwner owner;

  final TooltipWindowControllerDelegate _delegate;
  Rect _anchorRect;
  WindowPositioner _positioner;

  @override
  final BaseWindowController parent;

  @override
  void updatePosition({Rect? anchorRect, WindowPositioner? positioner}) {
    if (anchorRect != null) {
      _anchorRect = anchorRect;
    }
    if (positioner != null) {
      _positioner = positioner;
    }
    placeAtAnchor(
      parent: parent,
      anchorRect: _anchorRect,
      positioner: _positioner,
    );
  }

  @override
  void notifyGeometryChanged() {
    // The window shrink-wraps to its content after the first frame, so its
    // final height is only known here. Re-place it as the size settles.
    placeAtAnchor(
      parent: parent,
      anchorRect: _anchorRect,
      positioner: _positioner,
    );
    super.notifyGeometryChanged();
  }

  @override
  void handleCloseRequested() {
    destroy();
  }

  @override
  void onNativeDestroyed() {
    _delegate.onWindowDestroyed();
  }

  @override
  void dispose() {
    owner.unregister(viewId);
    super.dispose();
  }
}

class _GlfwPopupWindowController extends PopupWindowController
    with _GlfwCommon {
  _GlfwPopupWindowController({
    required this.owner,
    required this.api,
    required this.viewId,
    required PopupWindowControllerDelegate delegate,
    required this.parent,
    required Rect anchorRect,
    required WindowPositioner positioner,
  }) : _delegate = delegate,
       _anchorRect = anchorRect,
       _positioner = positioner,
       super.empty() {
    rootView = WidgetsBinding.instance.platformDispatcher.view(id: viewId)!;
  }

  @override
  final GlfwWindowingApi api;

  @override
  final int viewId;

  @override
  final GlfwWindowingOwner owner;

  final PopupWindowControllerDelegate _delegate;
  Rect _anchorRect;
  WindowPositioner _positioner;

  @override
  final BaseWindowController parent;

  @override
  Offset get offsetFromParent {
    final ({double x, double y}) parentPos = owner.api.getPosition(
      parent.rootView.viewId,
    );
    final ({double x, double y}) myPos = api.getPosition(viewId);
    return Offset(myPos.x - parentPos.x, myPos.y - parentPos.y);
  }

  @override
  void updatePosition({Rect? anchorRect, WindowPositioner? positioner}) {
    if (anchorRect != null) {
      _anchorRect = anchorRect;
    }
    if (positioner != null) {
      _positioner = positioner;
    }
    placeAtAnchor(
      parent: parent,
      anchorRect: _anchorRect,
      positioner: _positioner,
    );
  }

  @override
  void notifyGeometryChanged() {
    placeAtAnchor(
      parent: parent,
      anchorRect: _anchorRect,
      positioner: _positioner,
    );
    super.notifyGeometryChanged();
  }

  @override
  void handleCloseRequested() {
    destroy();
  }

  @override
  void onNativeDestroyed() {
    _delegate.onWindowDestroyed();
  }

  @override
  void dispose() {
    owner.unregister(viewId);
    super.dispose();
  }
}

class _GlfwSatelliteWindowController extends SatelliteWindowController
    with _GlfwCommon {
  _GlfwSatelliteWindowController({
    required this.owner,
    required this.api,
    required this.viewId,
    required SatelliteWindowControllerDelegate delegate,
    required BaseWindowController parent,
    String? title,
    BoxConstraints? constraints,
  }) : _delegate = delegate,
       _parent = parent,
       _title = title ?? '',
       _constraints = constraints,
       super.empty() {
    rootView = WidgetsBinding.instance.platformDispatcher.view(id: viewId)!;
    if (_constraints != null) {
      setConstraints(_constraints);
    }
  }

  @override
  final GlfwWindowingApi api;

  @override
  final int viewId;

  @override
  final GlfwWindowingOwner owner;

  final SatelliteWindowControllerDelegate _delegate;
  final BoxConstraints? _constraints;
  String _title;
  BaseWindowController _parent;

  @override
  BaseWindowController get parent => _parent;

  @override
  String get title => _title;

  @override
  bool get isActivated => api.isActivated(viewId);

  @override
  void setParent(BaseWindowController parent) {
    _parent = parent;
    api.setParent(viewId, parent.rootView.viewId);
  }

  @override
  void setSize(Size size) {
    api.setSize(viewId, size.width, size.height);
  }

  @override
  void setTitle(String title) {
    _title = title;
    api.setTitle(viewId, title);
  }

  @override
  void activate() {
    api.activate(viewId);
  }

  @override
  void handleCloseRequested() {
    _delegate.onWindowCloseRequested(this);
  }

  @override
  void onNativeDestroyed() {
    _delegate.onWindowDestroyed();
  }

  @override
  void dispose() {
    owner.unregister(viewId);
    super.dispose();
  }
}
