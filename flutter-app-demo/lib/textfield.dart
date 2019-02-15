import 'package:flutter/material.dart';
import 'ui/widgets.dart' as UI;

class TextFieldDemo extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: UI.AppBar(title: Text('TextField Demo')),
      body: Center(
        child: TextField(
          maxLines: 10,
        ),
      ),
    );
  }
}