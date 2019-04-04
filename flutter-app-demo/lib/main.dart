import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/foundation.dart' show debugDefaultTargetPlatformOverride;
import 'demos/demos.dart';
import 'utils.dart';


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
      // Since flutter tool is unable to generate AOT code for desktop,
      // our only option is to hide this banner and use JIT
      debugShowCheckedModeBanner: false,
      theme: getTheme(ThemeType.Base),
      initialRoute: '/',
      routes: {
        '/': (context) => Material(child: GetStartedPage()),
        '/demo': (context) => Material(child: DemoPage()),
      },
    );
  }
}

var cmd = 'git clone https://github.com/gliheng/flutter-app-template.git flutter_app\n./flutter_app/scripts/init.py';

class GetStartedPage extends StatelessWidget {
  final MethodChannel channel = MethodChannel('flutter/platform', JSONMethodCodec());

  void _showToast(BuildContext context, String text) {
    final scaffold = Scaffold.of(context);
    scaffold.showSnackBar(
      SnackBar(
        content: Text(text),
      ),
    );
  }

  Widget _buildBody(BuildContext context) {
    var theme = Theme.of(context);
    return Container(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: <Widget>[
          Expanded(
            flex: 5,
            child: Container(
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: Colors.red,
                  image: DecorationImage(
                    fit: BoxFit.cover,
                    alignment: Alignment.bottomCenter,
                    image: AssetImage('assets/header.png'),
                  ),
                ),
              ),
            ),
          ),
          Expanded(
            flex: 3,
            child: Column(
              mainAxisAlignment: MainAxisAlignment.spaceEvenly,
              children: <Widget>[
                Center(
                  child: SizedBox(
                    width: 700,
                    height: 100,
                    child: FlatButton(
                      color: Colors.black12,
                      child: Center(
                        child: Text(
                          cmd, style: TextStyle(
                            fontSize: 20,
                          ),
                        ),
                      ),
                      onPressed: () {
                        channel.invokeMethod('Clipboard.setData', {
                          'text': cmd,
                        });
                        _showToast(context, 'Copied to clipboard');
                      },
                    ),
                  ),
                ),
                Center(
                  child: RaisedButton(
                    color: theme.primaryColor,
                    padding: EdgeInsets.all(14.0),
                    textTheme: ButtonTextTheme.primary,
                    child: Text('Show Demos', style:TextStyle(
                      fontSize: 30
                    )),
                    onPressed: () {
                      Navigator.pushNamed(context, '/demo');
                    },
                  )
                ),
              ],
            )
          ),
        ],
      ),
    );
  }

  Widget build(BuildContext context) {
    return Scaffold(
      body: Builder(
        builder: (context) => _buildBody(context)
      )
    );
  }
}

class DemoPage extends StatefulWidget {
  @override
  _DemoPageState createState() => _DemoPageState();
}

class _DemoPageState extends State<DemoPage> {
  int currentIdx = 0;

  Widget _buildList() {
    return ListView.builder(itemBuilder: (BuildContext context, int i) {
      return ListTile(
        leading: Icon(demos[i].icon),
        selected: i == currentIdx,
        title: Text(demos[i].name),
        subtitle: Text(demos[i].description),
        onTap: () {
          setState(() {
            currentIdx = i;
          });
        },
      );
    }, itemCount: demos.length);
  }

  @override
  Widget build(BuildContext context) {
    return Row(
      children: <Widget>[
        SizedBox(
          width: 300,
          child: Container(
            decoration: BoxDecoration(
              color: Color.fromARGB(255, 50, 50, 50),
            ),
            child: Theme(
              data: getTheme(ThemeType.Inverted),
              child: _buildList()
            ),
          ),
        ),
        Expanded(
          child: demos[currentIdx].builder(context)
        ),
      ],
    );
  }
}