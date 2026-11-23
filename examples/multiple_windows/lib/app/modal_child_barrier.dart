// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// ignore_for_file: invalid_use_of_internal_member
// ignore_for_file: implementation_imports

import 'package:flutter/src/widgets/_window.dart';
import 'package:material_ui/material_ui.dart';

/// Dims a window while one of its modal dialog windows is open.
///
/// A parented [DialogWindowController] is modal to its parent, but it is a
/// separate native window, so the barrier that [showDialog] would paint inside
/// the parent's own view is missing. This covers the window with the same
/// `Colors.black54` barrier `showDialog` uses.
class ModalChildBarrier extends StatelessWidget {
  const ModalChildBarrier({
    super.key,
    required this.parent,
    required this.child,
  });

  /// The window whose modal children are watched.
  final BaseWindowController parent;

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final WindowRegistry registry = WindowRegistry.of(context);
    return ListenableBuilder(
      listenable: registry,
      builder: (BuildContext context, Widget? child) {
        final bool isModalOpen = registry.windows.any((WindowEntry entry) {
          final BaseWindowController controller = entry.controller;
          return controller is DialogWindowController &&
              controller.parent == parent;
        });
        // Keep the stack in place whether or not a modal is open: returning
        // the child bare first and wrapping it later would replace the widget
        // at this position, remount the whole window content, and drop the
        // state that owns the other windows' views (a popup would lose its
        // content while its native window stayed on screen).
        return Stack(
          fit: StackFit.expand,
          children: <Widget>[
            child!,
            if (isModalOpen)
              const ModalBarrier(color: Colors.black54, dismissible: false),
          ],
        );
      },
      child: child,
    );
  }
}
