import 'package:better_effect/better_effect.dart';

import 'app_effect.dart';
import 'app_failure.dart';
import 'cli_options.dart';
import 'cli_parser.dart';
import '../features/describe/describe_command.dart';
import '../features/doctor/doctor_command.dart';
import '../features/test_card/test_card_command.dart';
import '../application/terminal/terminal_ports.dart';
import '../features/init/init_command.dart';

abstract interface class CliApplication {
  AppEffect<int> run(List<String> arguments);
}

final class CliApplicationLive implements CliApplication {
  const CliApplicationLive();

  @override
  AppEffect<int> run(List<String> arguments) => .result((use) async {
    return switch (parseCli(arguments)) {
      HelpRequested() => _help(use),
      VersionRequested() => _version(use),
      InvalidArguments(:final message) => use.fail(CliFailure(message)),
      ParsedOptions(:final options) => await use.unwrap(
        switch (options.command) {
          Command.desc => use<DescribeCommand>().execute(options),
          Command.test => use<TestCardCommand>().execute(options),
          Command.init => use<InitCommand>().execute(options),
          Command.doctor => use<DoctorCommand>().execute(options),
        },
      ),
    };
  });

  int _help(EffectContext<AppFailure> use) {
    use<TerminalOutput>().write(helpText);
    return 0;
  }

  int _version(EffectContext<AppFailure> use) {
    use<TerminalOutput>().write('prt v$version');
    return 0;
  }
}
