import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

class EventChannelDemo extends StatefulWidget {
  @override
  _EventChannelDemoState createState() => _EventChannelDemoState();
}

class _EventChannelDemoState extends State<EventChannelDemo> {
  EventChannel channel = EventChannel('rust/random');
  List data = [];
  void listenStream() async {
    var stream = channel.receiveBroadcastStream(1234);
    await for (int value in stream) {
      setState(() {
        data.add(value);
      });
    }
  }

  @override
  void initState() {
    super.initState();
    listenStream();
  }

  @override
  Widget build(context) {
    return Scaffold(
      appBar: AppBar(title: Text('EventChannel Demo')),
      body: ListView.builder(
        itemBuilder: (context, i) {
          return ListTile(title: Text(data[i]));
        },
        itemCount: data.length,
      )
    );
  }
}
