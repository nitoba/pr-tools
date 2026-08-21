import 'dart:io';

Future<void> main(List<String> arguments) async {
  final nativeTarget = _nativeTarget();
  final target = switch (arguments) {
    [] => nativeTarget,
    [final value] => value,
    _ => _usage(),
  };
  if (target != nativeTarget) {
    stderr.writeln(
      'O alvo $target exige um host $target; o host atual é $nativeTarget.',
    );
    exitCode = 2;
    return;
  }

  final packageRoot = File.fromUri(Platform.script).parent.parent;
  final output = File(
    '${packageRoot.path}${Platform.pathSeparator}dist${Platform.pathSeparator}prt-$target${target == 'windows-x64' ? '.exe' : ''}',
  );
  await output.parent.create(recursive: true);
  final result = await Process.run(Platform.resolvedExecutable, [
    'compile',
    'exe',
    '-o',
    output.path,
    'bin/prt.dart',
  ], workingDirectory: packageRoot.path);
  stdout.write(result.stdout);
  stderr.write(result.stderr);
  if (result.exitCode != 0) {
    exitCode = result.exitCode;
    return;
  }
  stdout.writeln('Binário criado em ${output.path}');
}

String _nativeTarget() {
  final match = RegExp(r'on "([^_"]+)_([^"]+)"').firstMatch(Platform.version);
  final platform = match?.group(1);
  final architecture = match?.group(2);
  return switch ((platform, architecture)) {
    ('linux', 'x64') => 'linux-x64',
    ('linux', 'arm64') => 'linux-arm64',
    ('macos', 'arm64') => 'macos-arm64',
    ('windows', 'x64') => 'windows-x64',
    _ => throw UnsupportedError('Host Dart não suportado: ${Platform.version}'),
  };
}

Never _usage() {
  stderr.writeln('Uso: dart run tool/build.dart [alvo]');
  stderr.writeln('Alvos: linux-x64, linux-arm64, macos-arm64, windows-x64');
  exit(2);
}
