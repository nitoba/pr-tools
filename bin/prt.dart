import 'dart:io';

import 'package:better_effect/better_effect.dart';
import 'package:pr_tools/pr_tools.dart';
import 'package:terminice/terminice.dart';

Future<void> main(List<String> arguments) async {
  final runtime = await appModule.start();
  try {
    final parsed = parseCli(arguments);
    final result = switch (parsed) {
      ParsedOptions(:final options)
          when options.command == Command.desc ||
              options.command == Command.test =>
        await _runWithAzureScope(runtime, options, arguments),
      ParsedOptions(:final options) when options.command == Command.doctor =>
        await runtime.runWith(doctorExecutionModule(), runCli(arguments)),
      _ => await runtime.run(runCli(arguments)),
    };
    exitCode = result.fold((code) => code, (failure) {
      final ui = terminice.autoFallback;
      ui.error(failure.message);
      ui.detail('Use `prt --help` para ver as opções.');
      return failure.exitCode;
    });
  } finally {
    await runtime.close();
  }
}

Future<ResultDart<int, AppFailure>> _runWithAzureScope(
  Runtime runtime,
  CliOptions options,
  List<String> arguments,
) async {
  final scope = await runtime.run(azureExecutionModule(options));
  return scope.fold(
    (module) => runtime.runWith(module, runCli(arguments)),
    (failure) async => Failure<int, AppFailure>(failure),
  );
}
