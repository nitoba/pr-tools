import 'dart:io';

import 'package:test/test.dart';

void main() {
  test('runs version through the CLI entrypoint', () async {
    final result = await Process.run('dart', [
      'run',
      'bin/prt.dart',
      '--version',
    ]);

    expect(result.exitCode, 0, reason: result.stderr.toString());
    expect(result.stdout, contains('prt v4.0.7'));
  });
}
