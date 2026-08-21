import 'dart:convert';

import 'package:better_effect/better_effect.dart';
import 'package:pr_tools/src/app/app_effect.dart';
import 'package:pr_tools/src/app/app_failure.dart';
import 'package:pr_tools/src/app/cli_options.dart';
import 'package:pr_tools/src/application/config/config_models.dart';
import 'package:pr_tools/src/application/config/config_service.dart';
import 'package:pr_tools/src/application/terminal/terminal_ports.dart';
import 'package:pr_tools/src/infrastructure/config/config_service_live.dart';
import 'package:test/test.dart';

void main() {
  test('loads configuration with CLI, environment, dotenv, and JSON precedence', () async {
    final runtime = _Runtime(
      paths: _paths(),
      environment: const {
        'PR_AI_BASE_URL': 'https://env.example/v1',
        'PR_AI_REASONING': 'low',
        'PR_CODEX_MODEL': 'env-codex',
        'AZURE_PAT': 'env-pat',
      },
    );
    final files = _Files({
      runtime.paths.configFile: jsonEncode({
        'providers': ['opencode'],
        'baseUrl': 'https://json.example/v1',
        'compatibleModel': 'json-compatible',
        'compatibleReasoning': 'high',
        'codexModel': 'json-codex',
        'codexReasoning': 'medium',
        'opencodeModel': 'json-opencode',
        'opencodeReasoning': 'none',
        'reviewerDev': 'json-dev@example.com',
        'testTeam': 'JsonTeam',
        'apiKey': 'json-key',
      }),
      runtime.paths.envFile:
          'PR_AI_MODEL="dotenv-model"\nPR_REVIEWER_DEV="dotenv@example.com"\n',
      runtime.paths.templateFile: 'custom template\n',
    });

    final config = await _runLoad(
      runtime,
      files,
      const CliOptions(
        command: Command.desc,
        targets: [],
        provider: 'codex',
        model: 'cli-model',
        apiKey: 'cli-key',
        create: false,
        noCreate: false,
        dryRun: false,
        raw: false,
        copy: true,
      ),
    );

    expect(config.providers, ['codex']);
    expect(config.baseUrl, 'https://env.example/v1');
    expect(config.compatibleModel, 'cli-model');
    expect(config.codexModel, 'cli-model');
    expect(config.opencodeModel, 'cli-model');
    expect(config.compatibleReasoning, 'low');
    expect(config.codexReasoning, 'medium');
    expect(config.opencodeReasoning, 'none');
    expect(config.azurePat, 'env-pat');
    expect(config.reviewerDev, 'dotenv@example.com');
    expect(config.testTeam, 'JsonTeam');
    expect(config.apiKey, 'cli-key');
    expect(config.template, 'custom template');
  });

  test('initializes non-interactively and keeps secrets out of JSON', () async {
    final runtime = _Runtime(
      paths: _paths(),
      environment: const {'AZURE_DEVOPS_PAT': 'secret-pat'},
    );
    final files = _Files({
      runtime.paths.configFile: jsonEncode({
        'providers': ['opencode'],
        'opencodeModel': 'existing-model',
        'reviewerDev': 'reviewer@example.com',
      }),
    });

    final result = await _runInitialize(runtime, files);

    expect(result.cancelled, isFalse);
    expect(result.interactive, isFalse);
    expect(result.azurePatConfigured, isTrue);
    final saved = jsonDecode(files.values[runtime.paths.configFile]!) as Map;
    expect(saved['providers'], ['opencode']);
    expect(saved['opencodeModel'], 'existing-model');
    expect(saved.containsKey('azurePat'), isFalse);
    expect(
      files.values[runtime.paths.envFile],
      contains('AZURE_PAT="secret-pat"'),
    );
    expect(
      files.values[runtime.paths.templateFile],
      contains('Analise o diff'),
    );
    expect(files.modes[runtime.paths.configFile], 0x180);
    expect(files.modes[runtime.paths.envFile], 0x180);
    expect(files.modes[runtime.paths.templateFile], 0x180);
  });

  test('interactive initialization uses prompt contracts and preserves blank values', () async {
    final runtime = _Runtime(paths: _paths(), interactive: true);
    final files = _Files();
    final prompts = _Prompts(
      passwords: ['pat-value', ''],
      texts: ['', '', '', 'custom-codex', 'Area\\QA', 'DevOps', 'Agrotrace'],
      selects: ['codex', 'high'],
    );
    final result = await _runInitialize(runtime, files, prompts: prompts);

    expect(result.cancelled, isFalse);
    final saved = jsonDecode(files.values[runtime.paths.configFile]!) as Map;
    expect(saved['providers'], ['codex']);
    expect(saved['codexModel'], 'custom-codex');
    expect(saved['codexReasoning'], 'high');
    expect(saved['testAreaPath'], r'Area\QA');
    expect(
      files.values[runtime.paths.envFile],
      contains('AZURE_PAT="pat-value"'),
    );
    expect(
      prompts.messages,
      contains(
        'Email de review da sprint (opcional; Enter mantém o valor atual)',
      ),
    );
  });

  test('interactive cancellation does not write configuration', () async {
    final runtime = _Runtime(paths: _paths(), interactive: true);
    final files = _Files();
    final result = await _runInitialize(
      runtime,
      files,
      prompts: _Prompts(passwords: [null]),
    );

    expect(result.cancelled, isTrue);
    expect(files.values.containsKey(runtime.paths.configFile), isFalse);
  });
}

ConfigPaths _paths() => const ConfigPaths(
  directory: '/config/pr-tools',
  configFile: '/config/pr-tools/config.json',
  envFile: '/config/pr-tools/.env',
  templateFile: '/config/pr-tools/pr-template.md',
);

Future<Config> _runLoad(
  _Runtime runtime,
  _Files files,
  CliOptions options,
) async {
  final result = await _module(runtime, files).run(
    .result((use) async {
      return use.unwrap(use<ConfigService>().load(options));
    }),
  );
  return result.fold(
    (value) => value,
    (failure) => fail((failure as AppFailure).message),
  );
}

Future<ConfigInitialization> _runInitialize(
  _Runtime runtime,
  _Files files, {
  _Prompts? prompts,
}) async {
  final module = _module(runtime, files, prompts: prompts);
  final result = await module.run(
    .result((use) async {
      return use.unwrap(use<ConfigService>().initialize());
    }),
  );
  return result.fold(
    (value) => value,
    (failure) => fail((failure as AppFailure).message),
  );
}

Module _module(_Runtime runtime, _Files files, {_Prompts? prompts}) => Module([
  .instance<ConfigRuntime>(runtime),
  .instance<ConfigFileSystem>(files),
  if (prompts != null) .instance<PromptPort>(prompts),
  .provide<ConfigService>(ConfigServiceLive.new),
]);

final class _Runtime implements ConfigRuntime {
  _Runtime({
    required this.paths,
    this.environment = const {},
    this.interactive = false,
  });

  @override
  final ConfigPaths paths;

  @override
  final Map<String, String> environment;

  @override
  final bool interactive;
}

final class _Files implements ConfigFileSystem {
  _Files([Map<String, String>? initial]) : values = {...?initial};

  final Map<String, String> values;
  final Map<String, int> modes = {};

  @override
  AppEffect<bool> exists(String path) =>
      Effect<bool, AppFailure>.succeed(values.containsKey(path));

  @override
  AppEffect<String> createDirectory(String path) =>
      Effect<String, AppFailure>.succeed(path);

  @override
  AppEffect<String> read(String path) =>
      Effect<String, AppFailure>.succeed(values[path]!);

  @override
  AppEffect<String> write(String path, String contents, {required int mode}) {
    values[path] = contents;
    modes[path] = mode;
    return Effect<String, AppFailure>.succeed(path);
  }
}

final class _Prompts implements PromptPort {
  _Prompts({
    List<String?>? texts,
    List<String?>? passwords,
    List<String?>? selects,
  }) : _texts = [...?texts],
       _passwords = [...?passwords],
       _selects = [...?selects];

  final List<String?> _texts;
  final List<String?> _passwords;
  final List<String?> _selects;
  final List<String> messages = [];

  @override
  String? text({
    required String message,
    String? initialValue,
    String? placeholder,
    PromptValidator? validate,
  }) {
    messages.add(message);
    return _texts.removeAt(0);
  }

  @override
  String? password({required String message}) {
    messages.add(message);
    return _passwords.removeAt(0);
  }

  @override
  String? select({
    required String message,
    required List<PromptOption> options,
    String? initialValue,
  }) {
    messages.add(message);
    return _selects.removeAt(0);
  }
}
