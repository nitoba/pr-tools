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
    String? input,
  }) => Effect.result((use) async {
    final cancellation = use.cancellation;
    cancellation.throwIfCancelled();
    final process = await use.acquire(
      Effect.tryAsync(
        () => Process.start(command, arguments),
        onError: (error, _) => ProcessFailure(error.toString()),
      ),
      release: (process, _) {
        process.kill();
      },
    );
    unawaited(
      cancellation.whenCancelled.then((_) {
        process.kill();
      }),
    );
    final result = await use.tryAsync(() async {
      final stdout = process.stdout.transform(utf8.decoder).join();
      final stderr = process.stderr.transform(utf8.decoder).join();
      final outputFuture = Future.wait<Object>([
        process.exitCode,
        stdout,
        stderr,
      ]);
      final inputFuture = input == null
          ? Future<void>.value()
          : _writeInput(process, input);
      final resultFuture = () async {
        final output = await outputFuture;
        await inputFuture;
        return ProcessResult(
          exitCode: output[0] as int,
          stdout: (output[1] as String).trim(),
          stderr: (output[2] as String).trim(),
        );
      }();
      return timeout == null
          ? await resultFuture
          : await resultFuture.timeout(
              timeout,
              onTimeout: () {
                process.kill();
                return ProcessResult(
                  exitCode: 1,
                  stdout: '',
                  stderr: '',
                  error:
                      'Processo excedeu o timeout de ${timeout.inMilliseconds} ms.',
                );
              },
            );
    }, onError: (error, _) => ProcessFailure(error.toString()));
    cancellation.throwIfCancelled();
    return result;
  });
}

Future<void> _writeInput(Process process, String input) async {
  process.stdin.write(input);
  await process.stdin.close();
}
