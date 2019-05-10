import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'drag_window.dart';
import 'theme.dart';

class MenuBar extends StatefulWidget {
  MenuBar({ Key key, this.logo, this.menus = const <Menu>[] }): super(key: key);

  Widget logo;
  List<Menu> menus = [];

  @override
  _MenuBarState createState() => _MenuBarState();
}

class _MenuBarState extends State<MenuBar> {
  @override
  Widget build(BuildContext context) {
    var items = <Widget>[
      widget.logo ?? FlutterLogo(),
    ];
    items.addAll(widget.menus);
    items.add(Expanded(child: DragWindow()));
    items.add(WindowCtrls());
    
    var theme = DesktopTheme.of(context);
    return Container(
      height: theme.menuBarHeight,
      child: Container(
        color: Theme.of(context).primaryColor,
        child: Row(
          children: items,
        ),
      ),
    );
  }
}

class WindowCtrls extends StatefulWidget {
  @override
  _WindowCtrlsState createState() => _WindowCtrlsState();
}

class _WindowCtrlsState extends State<WindowCtrls> {
  bool maximized = false;

  final MethodChannel channel = MethodChannel('flutter-rs/window', JSONMethodCodec());
  
  @override
  void initState() {
    super.initState();

    channel.invokeMethod('isMaximized').then((v) {
      setState(() {
        maximized = v;
      });
    });
  }

  @override
  Widget build(BuildContext context) {
    return Material(
      color: Colors.transparent,
      child: Padding(
        padding: const EdgeInsets.only(left: 14),
        child: Row(children: <Widget>[
          IconButton(
            icon: Icon(Icons.minimize),
            iconSize: 20,
            onPressed: () {
                channel.invokeMethod('iconify');
            },
          ),
          IconButton(
            icon: maximized? Icon(Icons.filter_none) : Icon(Icons.crop_din),
            iconSize: 20,
            onPressed: () async {
              if (maximized) {
                channel.invokeMethod('restore');
              } else {
                channel.invokeMethod('maximize');
              }
              setState(() {
                maximized = !maximized;                
              });
            },
          ),
          IconButton(
            icon: Icon(Icons.close),
            iconSize: 20,
            onPressed: () {
                channel.invokeMethod('close');
            },
          ),
        ]),
      ),
    );
  }
}

typedef MenuItemBuilder<T> = List<PopupMenuEntry<T>> Function(BuildContext context);

class Menu<T> extends StatefulWidget {
  Menu({
    this.label,
    this.itemBuilder,
    this.onSelect,
  });

  @required String label;
  MenuItemBuilder<T> itemBuilder;
  void Function(T) onSelect;

  @override
  _MenuState createState() => _MenuState();
}

class _MenuState<T> extends State<Menu<T>> {
  bool menuOpen = false;

  List<PopupMenuEntry<T>> defaultItemBuilder(BuildContext context) => [];

  @override
  Widget build(BuildContext context) {
    return Material(
      color: Colors.transparent,
      child: PopupMenuButton<T>(
        offset: Offset(0, 50),
        onSelected: (T value) {
          if (value is String) {
            widget.onSelect ?? widget.onSelect(value);
          }
        },
        child: Container(
          child: Center(
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Text(widget.label),
            ),
          ),
        ),
 
        itemBuilder: widget.itemBuilder ?? defaultItemBuilder,
      ),
    );
  }
}