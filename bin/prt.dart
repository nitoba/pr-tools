import 'dart:async';
import 'dart:io';

import 'package:better_effect/better_effect.dart';
import 'package:pr_tools/pr_tools.dart';
import 'package:pr_tools/src/application/terminal/terminal_ports.dart';

Future<void> main(List<String> arguments) async {
  final runtime = await appModule.start();
  Future<void>? closing;

  void requestShutdown(ProcessSignal _) {
    closing ??= runtime.close(interruptAfterGracePeriod: true);
  }

  final signals = <StreamSubscription<ProcessSignal>>[
    ProcessSignal.sigint.watch().listen(requestShutdown),
    if (!Platform.isWindows)
      ProcessSignal.sigterm.watch().listen(requestShutdown),
  ];
  try {
    final parsed = parseCli(arguments);
    final exit = await switch (parsed) {
      ParsedOptions(:final options)
          when options.command == Command.desc ||
              options.command == Command.test =>
        _runWithAzureScope(runtime, options, arguments),
      ParsedOptions(:final options) when options.command == Command.doctor =>
        runtime.runExitWith(
          doctorExecutionModule(),
          runCli(arguments),
          executionLabel: 'cli.doctor',
        ),
      _ => runtime.runExit(runCli(arguments), executionLabel: 'cli'),
    };
    exitCode = await _exitCode(runtime, exit);
  } finally {
    for (final signal in signals) {
      await signal.cancel();
    }
    await (closing ?? runtime.close());
  }
}

Future<int> _exitCode(Runtime runtime, Exit<int, AppFailure> exit) async {
  switch (exit) {
    case ExitSuccess(:final value):
      return value;
    case ExitFailure(:final error):
      await runtime.run(
        Effect<Unit, AppFailure>.result((use) {
          final output = use<TerminalOutput>();
          output.writeError(error.message);
          output.detail('Use `prt --help` para ver as opções.');
          return unit;
        }),
      );
      return error.exitCode;
    case ExitInterrupted():
      return 130;
    case ExitDefect(:final defect, :final stackTrace):
      Error.throwWithStackTrace(defect, stackTrace);
  }
}

Future<Exit<int, AppFailure>> _runWithAzureScope(
  Runtime runtime,
  CliOptions options,
  List<String> arguments,
) async {
  final scope = await runtime.runExit(
    azureExecutionModule(options),
    executionLabel: 'cli.azure.setup',
  );
  return switch (scope) {
    ExitSuccess(:final value) => runtime.runExitWith(
      value,
      runCli(arguments),
      executionLabel: 'cli.${options.command.name}',
    ),
    ExitFailure(:final error) => ExitFailure<int, AppFailure>(error),
    ExitInterrupted() => const ExitInterrupted<int, AppFailure>(),
    ExitDefect(:final defect, :final stackTrace) => Error.throwWithStackTrace(
      defect,
      stackTrace,
    ),
  };
}
