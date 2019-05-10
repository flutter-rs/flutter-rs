import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

class DesktopThemeData extends Diagnosticable {
  DesktopThemeData({ this.menuBarHeight });

  final double menuBarHeight;
}

class DesktopTheme extends InheritedWidget {
  DesktopTheme({
    Key key, Widget child,
    DesktopThemeData data
  }): data = data ?? DesktopThemeData(
    menuBarHeight: 40,
  ), super(key: key, child: child);
  
  DesktopThemeData data;

  static DesktopThemeData of(BuildContext context) {
    return (context.inheritFromWidgetOfExactType(DesktopTheme) as DesktopTheme).data;
  }

  @override
  bool updateShouldNotify(DesktopTheme old) => data != old.data;
}