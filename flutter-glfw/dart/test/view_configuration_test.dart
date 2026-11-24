import 'package:flutter/rendering.dart';
import 'package:flutter_glfw_windowing/src/windowing.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const ViewConfiguration base = ViewConfiguration(
    physicalConstraints: BoxConstraints.tightFor(width: 400, height: 300),
    logicalConstraints: BoxConstraints.tightFor(width: 400, height: 300),
    devicePixelRatio: 1,
  );

  test('a following view is laid out with unbounded constraints', () {
    final ViewConfiguration configuration = windowConfigurationFor(
      base,
      followsContent: true,
      shrinkWrapped: true,
    );

    expect(configuration.logicalConstraints.maxWidth, double.infinity);
    expect(configuration.logicalConstraints.maxHeight, double.infinity);
    expect(configuration.physicalConstraints.maxWidth, double.infinity);
    expect(configuration.devicePixelRatio, 1);
  });

  test('a handed-off shrink-wrapped view keeps the window and gains slack', () {
    final ViewConfiguration configuration = windowConfigurationFor(
      base,
      followsContent: false,
      shrinkWrapped: true,
    );

    expect(configuration.logicalConstraints.maxWidth, 401);
    expect(configuration.logicalConstraints.maxHeight, 301);
    expect(configuration.physicalConstraints, base.physicalConstraints);
    expect(configuration.devicePixelRatio, 1);
  });

  test('an ordinary view is unchanged', () {
    expect(
      windowConfigurationFor(base, followsContent: false, shrinkWrapped: false),
      base,
    );
  });
}
