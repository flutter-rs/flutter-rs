import 'package:flutter/material.dart' hide AppBar;
import 'package:flutter/material.dart' as M show AppBar;
import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';

class AppBar extends StatelessWidget implements PreferredSizeWidget {
  final MethodChannel channel = MethodChannel('flutter-rs/window', JSONMethodCodec());
  final Widget title;

  AppBar({this.title});

  void onPanStart(DragStartDetails details) async {
    channel.invokeMethod('start_drag');
  }

  void onPanEnd(DragEndDetails details) {
    channel.invokeMethod('end_drag');
  }

  Size get preferredSize {
    return Size.fromHeight(40.0);
  }

  @override
  Widget build(BuildContext context) {
    return M.AppBar(
      title: GestureDetector(
        behavior: HitTestBehavior.opaque,
        child: Container(
          constraints: BoxConstraints.expand(),
          child: Align(
            alignment: Alignment.centerLeft,
            child: title,
          ),
        ),
        onPanStart: onPanStart,
        onPanEnd: onPanEnd,
      )
    );
  }
}