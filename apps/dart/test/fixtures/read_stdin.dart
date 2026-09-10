import 'dart:convert';
import 'dart:io';

Future<void> main() async =>
    stdout.write(await stdin.transform(utf8.decoder).join());
