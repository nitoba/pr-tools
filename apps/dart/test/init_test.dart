import 'package:better_effect/better_effect.dart';
import 'package:pr_tools/src/app/app_effect.dart';
import 'package:pr_tools/src/app/app_failure.dart';
import 'package:pr_tools/src/app/cli_options.dart';
import 'package:pr_tools/src/application/config/config_models.dart';
import 'package:pr_tools/src/application/config/config_service.dart';
import 'package:pr_tools/src/application/terminal/terminal_ports.dart';
import 'package:pr_tools/src/features/init/init_command.dart';
import 'package:test/test.dart';

void main() {
  test('resolves init command dependencies from module bindings', () async {
    final output = _Output();
    final paths = const ConfigPaths(
      directory: '/config/pr-tools',
      configFile: '/config/pr-tools/config.json',
      envFile: '/config/pr-tools/.env',
      templateFile: '/config/pr-tools/pr-template.md',
    );
    final result =
        await Module([
          .instance<ConfigRuntime>(_Runtime(paths)),
          .instance<TerminalOutput>(output),
          .instance<ConfigService>(
            _ConfigServiceFake(
              ConfigInitialization(
                paths: paths,
                interactive: true,
                azurePatConfigured: true,
              ),
            ),
          ),
          .provide<InitCommand>(InitCommandLive.new),
        ]).run(
          .result((use) async {
            return use.unwrap(use<InitCommand>().execute(_options()));
          }),
        );

    expect(
      result.fold(
        (value) => value,
        (failure) => fail((failure as AppFailure).message),
      ),
      0,
    );
    expect(output.messages.first, 'heading:Configuração');
    expect(output.messages.join('\n'), contains('PAT Azure salvo'));
    expect(
      output.messages.last,
      'success:Pronto. Execute `prt desc --dry-run`.',
    );
  });

  test('reports cancellation without failing the command', () async {
    final output = _Output();
    final paths = const ConfigPaths(
      directory: '/config/pr-tools',
      configFile: '/config/pr-tools/config.json',
      envFile: '/config/pr-tools/.env',
      templateFile: '/config/pr-tools/pr-template.md',
    );
    final result =
        await Module([
          .instance<ConfigRuntime>(_Runtime(paths)),
          .instance<TerminalOutput>(output),
          .instance<ConfigService>(
            _ConfigServiceFake(
              ConfigInitialization.cancelled(paths: paths, interactive: true),
            ),
          ),
          .provide<InitCommand>(InitCommandLive.new),
        ]).run(
          .result((use) async {
            return use.unwrap(use<InitCommand>().execute(_options()));
          }),
        );

    expect(
      result.fold(
        (value) => value,
        (failure) => fail((failure as AppFailure).message),
      ),
      0,
    );
    expect(output.messages.last, 'warning:Operação cancelada.');
  });
}

CliOptions _options() => const CliOptions(
  command: Command.init,
  targets: [],
  create: false,
  noCreate: false,
  dryRun: false,
  raw: false,
  copy: true,
);

final class _Runtime implements ConfigRuntime {
  const _Runtime(this.paths);

  @override
  final ConfigPaths paths;

  @override
  Map<String, String> get environment => const {};

  @override
  bool get interactive => true;
}

final class _ConfigServiceFake implements ConfigService {
  const _ConfigServiceFake(this.initialization);

  final ConfigInitialization initialization;

  @override
  AppEffect<Config> load(CliOptions options) =>
      Effect<Config, AppFailure>.fail(const CliFailure('not used'));

  @override
  AppEffect<ConfigInitialization> initialize() =>
      Effect<ConfigInitialization, AppFailure>.succeed(initialization);
}

final class _Output implements TerminalOutput {
  final List<String> messages = [];

  @override
  void heading(String title, {String? detail}) =>
      messages.add('heading:$title${detail == null ? '' : ':$detail'}');

  @override
  void card(String title, String content) =>
      messages.add('card:$title:$content');

  @override
  void write(String message) => messages.add(message);

  @override
  void writeError(String message) => messages.add(message);

  @override
  void info(String message) => messages.add('info:$message');

  @override
  void success(String message) => messages.add('success:$message');

  @override
  void warning(String message) => messages.add('warning:$message');

  @override
  void detail(String message) => messages.add('detail:$message');
}
