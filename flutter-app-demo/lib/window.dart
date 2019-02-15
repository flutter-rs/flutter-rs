import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'ui/widgets.dart' as UI;

class WindowDemo extends StatelessWidget {
  final MethodChannel channel = MethodChannel('flutter-rs/window', JSONMethodCodec());

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: UI.AppBar(title: Text('Window Demo')),
      body: Center(
        child: Wrap(
          spacing: 10.0,
          children: <Widget>[
            RaisedButton(
              child: Text('Maximize'),
              onPressed: () {
                channel.invokeMethod('maximize');
              },
            ),
            RaisedButton(
              child: Text('Iconify'),
              onPressed: () {
                channel.invokeMethod('iconify');
              },
            ),
            RaisedButton(
              child: Text('Restore'),
              onPressed: () {
                channel.invokeMethod('restore');
              },
            ),
            RaisedButton(
              child: Text('Close'),
              onPressed: () {
                channel.invokeMethod('close');
              },
            ),
          ],
        ),
      ),
    );
  }
}