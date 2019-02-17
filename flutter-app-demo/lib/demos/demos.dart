import 'package:flutter/material.dart';
import 'method_channel.dart';
import 'event_channel.dart';
import 'file_dialog.dart';
import 'textfield.dart';
import 'window.dart';

class Demo {
  String name;
  String description;
  IconData icon;
  Function(BuildContext) builder;
  Demo(this.name, this.description, this.icon, this.builder);
}

List<Demo> demos = [
  Demo(
    'MethodChannel',
    'Use MethodChannel to invoke rust',
    Icons.toll,
    (BuildContext context) => MethodChannelDemo()),
  Demo(
    'EventChannel',
    'Use EventChannel to listen to rust stream',
    Icons.layers,
    (BuildContext context) => EventChannelDemo()),
  Demo(
    'File Dialogs',
    'Open system file dialogs',
    Icons.account_box,
    (BuildContext context) => FileDialogDemo()),
  Demo(
    'TextField',
    'TextField Demo',
    Icons.input,
    (BuildContext context) => TextFieldDemo()),
  Demo(
    'Window',
    'Control native window',
    Icons.laptop_windows,
    (BuildContext context) => WindowDemo()),
];