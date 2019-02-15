import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'ui/widgets.dart' as UI;

class FileDialogDemo extends StatefulWidget {
  @override
  _FileDialogDemoState createState() => _FileDialogDemoState();
}

class _FileDialogDemoState extends State<FileDialogDemo> {
  MethodChannel channel = MethodChannel('flutter-rs/dialog', JSONMethodCodec());

  String ret;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: UI.AppBar(title: Text('File Dialog Demo')),
      body: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            Wrap(
              spacing: 10.0,
              children: <Widget>[
                RaisedButton(child: Text('Open File'), onPressed: () async {
                  var s = await channel.invokeMethod('open_file_dialog', {
                    'title': 'Open file',
                    'path': '/',
                    'filter': [['*.jpg', '*.png'], 'Image Files'],
                  });
                  setState(() {
                    ret = s;
                  });
                }),
                RaisedButton(child: Text('Message Box'), onPressed: () {
                  channel.invokeMethod('message_box_ok', {
                    'title': 'Hello',
                    'message': 'How are you today?',
                  });
                }),
              ]
            ),
            Text(ret ?? ''),
          ],
        ),
      )
    );
  }
}