import 'package:flutter/material.dart';
import '../ui/widgets.dart' as UI;

class TextFieldDemo extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: UI.AppBar(title: Text('TextField Demo')),
      body: Container(
        child: Padding(
          padding: const EdgeInsets.all(20.0),
          child: TextField(
            autofocus: true,
            decoration: InputDecoration(
              border: OutlineInputBorder(),
            ),
            maxLines: 20,
          ),
        ),
      ),
    );
  }
}