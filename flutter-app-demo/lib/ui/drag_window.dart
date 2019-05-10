import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

class DragWindow extends StatelessWidget {
  DragWindow({Key key, this.child}): super(key: key);

  Widget child;

  final MethodChannel channel = MethodChannel('flutter-rs/window', JSONMethodCodec());
  void onPanStart(DragStartDetails details) async {
    channel.invokeMethod('start_drag');
  }

  void onPanEnd(DragEndDetails details) {
    channel.invokeMethod('end_drag');
  }

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      child: child,
      // child: Container(
      //   constraints: BoxConstraints.expand(),
      //   child: child,
      // ),
      onPanStart: onPanStart,
      onPanEnd: onPanEnd,
    );
  }
}