import 'package:flutter/material.dart';
import 'dart:io' show Platform;

enum ThemeType {
  Base,
  Inverted,
}

ThemeData getTheme(ThemeType type) {
  var fontFamily = getFont();
  if (type == ThemeType.Inverted) {
    return ThemeData(
      primarySwatch: Colors.blue,
      accentColor: Colors.lightBlue.shade200,
      brightness: Brightness.dark,
      fontFamily: fontFamily,
    );
  }

  return ThemeData(
    primarySwatch: Colors.blue,
    brightness: Brightness.light,
    fontFamily: fontFamily,
  );
}

String getFont() {
  if (Platform.isLinux) {
    return 'Ubuntu';
  } else if (Platform.isWindows) {
  } else if (Platform.isMacOS) {
  }
  return null;
}