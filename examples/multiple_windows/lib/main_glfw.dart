// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// ignore_for_file: implementation_imports, invalid_use_of_internal_member

import 'package:flutter/src/foundation/_features.dart' show isWindowingEnabled;
import 'package:flutter/widgets.dart';
import 'package:flutter_glfw_windowing/flutter_glfw_windowing.dart';

import 'main.dart' as example;

/// Entrypoint for the flutter-rs (GLFW) runner.
///
/// Flutter 3.50 gates the windowing API behind a compile-time feature flag that
/// stable `flutter build` cannot set. The flag is a plain mutable top-level
/// variable, so the embedder enables it at runtime and installs its own
/// `WindowingOwner` before the example's `main()` builds its first controller.
void main() {
  WidgetsFlutterBinding.ensureInitialized();
  isWindowingEnabled = true;
  WidgetsBinding.instance.windowingOwner = GlfwWindowingOwner();
  example.main();
}
