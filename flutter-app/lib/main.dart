import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart' show debugDefaultTargetPlatformOverride;
import 'method_channel.dart';
import 'event_channel.dart';
import 'file_dialog.dart';

void main() {
  // Override is necessary to prevent Unknown platform' flutter startup error.
  debugDefaultTargetPlatformOverride = TargetPlatform.android;
  runApp(MyApp());
}

class MyApp extends StatelessWidget {
  // This widget is the root of your application.
  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Flutter Demo',
      theme: ThemeData(
        primarySwatch: Colors.blue,
      ),
      home: Material(child: MyHomePage()),
    );
  }
}

class Demo {
  String name;
  String description;
  Function(BuildContext) builder;
  Demo(this.name, this.description, this.builder);
}

List<Demo> demos = [
  Demo(
    'MethodChannel',
    'Use MethodChannel to invoke rust',
    (BuildContext context) => MethodChannelDemo()),
  Demo(
    'EventChannel',
    'Use EventChannel to listen to rust stream',
    (BuildContext context) => EventChannelDemo()),
  Demo(
    'File Dialogs',
    'Open system file dialogs',
    (BuildContext context) => FileDialogDemo()),
];

class MyHomePage extends StatefulWidget {
  @override
  _MyHomePageState createState() => _MyHomePageState();
}

class _MyHomePageState extends State<MyHomePage> {
  int currentIdx = 0;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: <Widget>[
        SizedBox(
          width: 300,
          child: ListView.builder(itemBuilder: (BuildContext context, int i) {
            return ListTile(
              selected: i == currentIdx,
              title: Text(demos[i].name),
              subtitle: Text(demos[i].description),
              onTap: () {
                setState(() {
                  currentIdx = i;
                });
              },
            );
          }, itemCount: demos.length)
        ),
        Expanded(
          child: demos[currentIdx].builder(context)
        ),
      ],
    );
  }
}