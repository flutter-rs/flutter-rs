import 'package:flutter/material.dart' hide AppBar;
import 'package:flutter/material.dart' as M show AppBar;
import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';

class AppBar extends StatelessWidget implements PreferredSizeWidget {
  final MethodChannel channel = MethodChannel('flutter-rs/window', JSONMethodCodec());
  final Widget title;
  Offset startPos;

  AppBar({this.title});

  void onPanStart(DragStartDetails details) async {
    startPos = details.globalPosition;
  }

  void onPanUpdate(DragUpdateDetails details) async {
    // globalPosition is relative to the top-left corner of client area
    // We need to keep globalPosition the same when dragging the window
    var deltaX = details.globalPosition.dx - startPos.dx;
    var deltaY = details.globalPosition.dy - startPos.dy;
  
    Map<String, dynamic> winPos = await channel.invokeMethod('get_pos');
    var pos = winPos.cast<String, int>();

    channel.invokeMethod('set_pos', {
      'x': pos['x'].toDouble() + deltaX,
      'y': pos['y'].toDouble() + deltaY,
    });
  }

  void onPanEnd(DragEndDetails details) {
    startPos = null;
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
        onPanUpdate: onPanUpdate,
        onPanEnd: onPanEnd,
      )
    );
  }
}