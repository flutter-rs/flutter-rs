import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

class MethodChannelDemo extends StatefulWidget {
  @override
  _MethodChannelDemoState createState() => _MethodChannelDemoState();
}

class _MethodChannelDemoState extends State<MethodChannelDemo> {
  MethodChannel channel = MethodChannel('rust/calc');
  int ret;
  String errorCode;
  String errorMessage;

  @override
  Widget build(context) {
    var label;
    if (ret != null) {
      label = Text('$ret');
    } else if (errorCode != null) {
      label = Text('ErrorCode: $errorCode,  ErrorMessage: $errorMessage', style: TextStyle(
        color: Colors.red
      ));
    } else {
      label = Text('');
    }

    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          Container(
            width: 150,
            child: TextField(
              autofocus: true,
              textAlign: TextAlign.center,
              onChanged: (s) async {
                try {
                  var i = int.parse(s);
                  final int n = await channel.invokeMethod('fibonacci', i);
                  setState(() {
                    ret = n;
                    errorCode = null;
                    errorMessage = null;
                  });
                } on PlatformException catch (e) {
                  setState(() {
                    ret = null;
                    errorCode = e.code;
                    errorMessage = e.message;
                  });
                } catch (e) {
                  setState(() {
                    ret = null;
                    errorCode = null;
                    errorMessage = null;
                  });
                }
              },
            ),
          ),
          label,
        ],
      ),
    );
  }
}
