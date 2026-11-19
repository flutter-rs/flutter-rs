import 'dart:ffi';

import 'package:flutter_glfw_windowing/src/api.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('FgWindowParams layout matches the Rust repr(C) struct', () {
    expect(sizeOf<FgWindowParams>(), 56);
  });

  test('FgWindowingApi layout matches the Rust repr(C) table', () {
    // version + size (8 bytes) followed by 20 function pointers.
    expect(sizeOf<FgWindowingApi>(), 8 + 25 * sizeOf<Pointer<Void>>());
  });
}
