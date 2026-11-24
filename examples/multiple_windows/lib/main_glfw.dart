// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:flutter_glfw_windowing/flutter_glfw_windowing.dart';

import 'main.dart' as example;

/// Entrypoint for the flutter-rs (GLFW) runner.
///
/// Flutter 3.50 gates the windowing API behind a compile-time feature flag that
/// stable `flutter build` cannot set, and installs its platform windowing
/// owner as the binding initializes. [GlfwWindowingBinding] replaces both: it
/// enables the flag and hands the binding a `GlfwWindowingOwner`, which is the
/// only owner that works on the generic embedder this runner loads.
void main() {
  GlfwWindowingBinding();
  example.main();
}
