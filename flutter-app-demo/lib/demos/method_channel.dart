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
    var theme = Theme.of(context).textTheme;
    return Scaffold(
      appBar: AppBar(title: Text('MethodChannel Demo')),
      body: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            Text('Calc Fibonacci', style: theme.title),
            Container(
              width: 150,
              child: TextField(
                autofocus: true,
                textAlign: TextAlign.center,
                onChanged: (s) async {
                  try {
                    final int n = await channel.invokeMethod('fibonacci', s);
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
                  }
                },
              ),
            ),
            label,
          ],
        ),
      )
    );
  }
}
