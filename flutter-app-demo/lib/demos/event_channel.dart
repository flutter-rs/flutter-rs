import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';


class EventChannelDemo extends StatefulWidget {
  @override
  _EventChannelDemoState createState() => _EventChannelDemoState();
}

class _EventChannelDemoState extends State<EventChannelDemo> {
  EventChannel channel = EventChannel('rust/msg_stream');
  List<String> data = [];
  StreamSubscription sub;

  void listenStream() async {
    var stream = channel.receiveBroadcastStream(1234);
    sub = stream.listen(onData, onDone: onDone, onError: onError);

    setState(() {
      data = [];
    });
  }

  onData(dynamic v) {
    print('onData $v');
    setState(() {
      data.add(v);
    });
  }

  onError(dynamic error) {
    print('onError $error');
  }

  onDone() {
    print('onDone');
  }

  cancelStream() async {
    if (sub != null) {
      await sub.cancel();
      if (this.mounted) {
        setState(() {});
      }
    }
  }

  @override
  void dispose() {
    cancelStream();
    super.dispose();
  }

  @override
  Widget build(context) {
    var status;
    if (sub != null && !sub.isPaused) {
      status = 'Listening';
    } else {
      status = 'Not listening';
    }

    return Scaffold(
      appBar: AppBar(title: Text('EventChannel Demo')),
      body: ListView.builder(
        itemBuilder: (context, i) {
          return ListTile(title: Text(data[i].toString()));
        },
        itemCount: data.length,
      ),
      bottomNavigationBar: BottomAppBar(
        child: Row(children: <Widget>[
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8.0),
            child: RaisedButton(child: Text("Listen"), onPressed: () {
              listenStream();
            }),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8.0),
            child: RaisedButton(child: Text("Cancel"), onPressed: () {
              cancelStream();
            }),
          ),    
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8.0),
            child: Text('Status: $status'),
          )      
        ])
      ),
    );
  }
}
