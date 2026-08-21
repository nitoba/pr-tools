import 'dart:io';

import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import '../../application/clipboard/clipboard.dart';

final class NativeClipboard implements Clipboard {
  const NativeClipboard();

  @override
  AppEffect<bool> copy(String value) => Effect.result((use) async {
    for (final command in _commands) {
      final copied = await use.unwrap(_copy(command, value).either());
      if (copied.getOrNull() == true) return true;
    }
    return false;
  });

  AppEffect<bool> _copy(List<String> command, String value) =>
      Effect.tryAsync(() async {
        final process = await Process.start(
          command.first,
          command.skip(1).toList(),
        );
        process.stdin.write(value);
        await process.stdin.close();
        return await process.exitCode == 0;
      }, onError: (error, _) => ClipboardFailure(error.toString()));
}

const _commands = <List<String>>[
  ['pbcopy'],
  ['wl-copy'],
  ['xclip', '-selection', 'clipboard'],
  ['xsel', '--clipboard', '--input'],
  ['clip'],
];

final class ClipboardFailure extends AppFailure {
  const ClipboardFailure(String message) : super(message, 1);
}
