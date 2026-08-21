import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import '../../application/process/process_runner.dart';

final class ProcessRunnerLive implements ProcessRunner {
  const ProcessRunnerLive();

  @override
  AppEffect<ProcessResult> run(
    String command,
    List<String> arguments, {
    Duration? timeout,
  }) => Effect<ProcessResult, AppFailure>.tryAsync(() async {
    final process = await Process.start(command, arguments);
    final stdout = process.stdout.transform(utf8.decoder).join();
    final stderr = process.stderr.transform(utf8.decoder).join();
    final outputFuture = Future.wait<Object>([
      process.exitCode,
      stdout,
      stderr,
    ]);
    final output = timeout == null
        ? await outputFuture
        : await outputFuture.timeout(
            timeout,
            onTimeout: () {
              process.kill();
              return <Object>[
                1,
                '',
                '',
                'Processo excedeu o timeout de ${timeout.inMilliseconds} ms.',
              ];
            },
          );
    return ProcessResult(
      exitCode: output[0] as int,
      stdout: (output[1] as String).trim(),
      stderr: (output[2] as String).trim(),
      error: output.length > 3 ? output[3] as String : null,
    );
  }, onError: (error, _) => ProcessFailure(error.toString()));
}
