import 'dart:convert';
import 'dart:io';

import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import '../../app/cli_options.dart';
import '../../application/config/config_defaults.dart';
import '../../application/config/config_models.dart';
import '../../application/config/config_service.dart';
import '../../application/config/config_validation.dart';
import '../../application/terminal/terminal_ports.dart';

final class ConfigRuntimeLive implements ConfigRuntime {
  const ConfigRuntimeLive();

  @override
  ConfigPaths get paths {
    final environment = Platform.environment;
    final configHome =
        environment['XDG_CONFIG_HOME'] ??
        _joinPath(
          environment['HOME'] ?? environment['USERPROFILE'] ?? '.',
          '.config',
        );
    final directory = _joinPath(configHome, 'pr-tools');
    return ConfigPaths(
      directory: directory,
      configFile: _joinPath(directory, 'config.json'),
      envFile: _joinPath(directory, '.env'),
      templateFile: _joinPath(directory, 'pr-template.md'),
    );
  }

  @override
  Map<String, String> get environment => Platform.environment;

  @override
  bool get interactive => stdin.hasTerminal && stdout.hasTerminal;
}

final class ConfigFileSystemLive implements ConfigFileSystem {
  const ConfigFileSystemLive();

  @override
  AppEffect<bool> exists(String path) => Effect.tryAsync(
    () => File(path).exists(),
    onError: (error, _) => _fileSystemFailure(error),
  );

  @override
  AppEffect<String> createDirectory(String path) => Effect.tryAsync(() async {
    await Directory(path).create(recursive: true);
    return path;
  }, onError: (error, _) => _fileSystemFailure(error));

  @override
  AppEffect<String> read(String path) => Effect.tryAsync(
    () => File(path).readAsString(),
    onError: (error, _) => _fileSystemFailure(error),
  );

  @override
  AppEffect<String> write(
    String path,
    String contents, {
    required int mode,
  }) => Effect.tryAsync(() async {
    final file = File(path);
    await file.parent.create(recursive: true);
    await file.writeAsString(contents, flush: true);
    if (!Platform.isWindows) {
      final result = await Process.run('chmod', [mode.toRadixString(8), path]);
      if (result.exitCode != 0) {
        throw FileSystemException('Não foi possível proteger o arquivo.', path);
      }
    }
    return path;
  }, onError: (error, _) => _fileSystemFailure(error));
}

final class ConfigServiceLive implements ConfigService {
  const ConfigServiceLive();

  @override
  AppEffect<Config> load(CliOptions options) => .result((use) async {
    final runtime = use<ConfigRuntime>();
    final files = use<ConfigFileSystem>();
    final envExists = await use.unwrap(files.exists(runtime.paths.envFile));
    final dotEnv = envExists
        ? _parseDotEnv(await use.unwrap(files.read(runtime.paths.envFile)))
        : <String, String>{};
    final configExists = await use.unwrap(
      files.exists(runtime.paths.configFile),
    );
    final json = configExists
        ? await use.unwrap(
            _readJson(
              await use.unwrap(files.read(runtime.paths.configFile)),
              runtime.paths.configFile,
            ),
          )
        : <String, Object?>{};
    final configuredTemplate = json['template'];
    final templateExists = await use.unwrap(
      files.exists(runtime.paths.templateFile),
    );
    final template =
        configuredTemplate is String && configuredTemplate.trim().isNotEmpty
        ? configuredTemplate
        : templateExists
        ? (await use.unwrap(files.read(runtime.paths.templateFile))).trim()
        : defaultTemplate;
    return use.unwrap(
      _loadConfig(
        options,
        runtime.environment,
        dotEnv,
        json,
        template.isEmpty ? defaultTemplate : template,
      ),
    );
  });

  @override
  AppEffect<ConfigInitialization> initialize() => .result((use) async {
    final runtime = use<ConfigRuntime>();
    final files = use<ConfigFileSystem>();
    final paths = runtime.paths;
    await use.unwrap(files.createDirectory(paths.directory));
    final configExists = await use.unwrap(files.exists(paths.configFile));
    final existing = configExists
        ? await use.unwrap(
            _readJson(
              await use.unwrap(files.read(paths.configFile)),
              paths.configFile,
            ),
          )
        : <String, Object?>{};
    final envExists = await use.unwrap(files.exists(paths.envFile));
    final existingDotEnv = envExists
        ? _parseDotEnv(await use.unwrap(files.read(paths.envFile)))
        : <String, String>{};
    final environment = runtime.environment;

    var provider = await use.unwrap(_existingProvider(existing));
    var baseUrl = _stringValue(existing['baseUrl']) ?? defaultBaseUrl;
    var compatibleModel =
        _stringValue(existing['compatibleModel']) ?? defaultCompatibleModel;
    var compatibleReasoning = await use.unwrap(
      _reasoningResult(
        existing['compatibleReasoning'],
        compatibleReasoningDefault,
      ),
    );
    var codexModel = _stringValue(existing['codexModel']) ?? codexModelDefault;
    var codexThinking = await use.unwrap(
      _reasoningResult(existing['codexReasoning'], codexReasoningDefault),
    );
    var opencodeModel =
        _stringValue(existing['opencodeModel']) ?? opencodeModelDefault;
    var opencodeThinking = await use.unwrap(
      _reasoningResult(existing['opencodeReasoning'], opencodeReasoningDefault),
    );
    var apiKey = _stringValue(existing['apiKey']) ?? '';
    var azurePat =
        environment['AZURE_DEVOPS_PAT'] ??
        environment['AZURE_PAT'] ??
        existingDotEnv['AZURE_DEVOPS_PAT'] ??
        existingDotEnv['AZURE_PAT'] ??
        '';
    var reviewerDev =
        _stringValue(existing['reviewerDev']) ??
        existingDotEnv['PR_REVIEWER_DEV'] ??
        '';
    var reviewerSprint =
        _stringValue(existing['reviewerSprint']) ??
        existingDotEnv['PR_REVIEWER_SPRINT'] ??
        '';
    var testAreaPath =
        _stringValue(existing['testAreaPath']) ??
        existingDotEnv['TEST_CARD_AREA_PATH'] ??
        '';
    var testAssignedTo =
        _stringValue(existing['testAssignedTo']) ??
        existingDotEnv['TEST_CARD_ASSIGNED_TO'] ??
        '';
    var testTeam =
        _stringValue(existing['testTeam']) ??
        existingDotEnv['TEST_CARD_TEAM'] ??
        'DevOps';
    var testProgram =
        _stringValue(existing['testProgram']) ??
        existingDotEnv['TEST_CARD_PROGRAM'] ??
        'Agrotrace';

    if (runtime.interactive) {
      final prompts = use<PromptPort>();
      ({bool cancelled, String value}) textValue({
        required String message,
        String? initialValue,
        String? placeholder,
        PromptValidator? validate,
      }) {
        final value = prompts.text(
          message: message,
          initialValue: initialValue,
          placeholder: placeholder,
          validate: validate,
        );
        return value == null
            ? (cancelled: true, value: '')
            : (cancelled: false, value: value);
      }

      final patValue = prompts.password(
        message: 'Azure DevOps PAT (Enter para manter o atual)',
      );
      if (patValue == null) {
        return ConfigInitialization.cancelled(paths: paths, interactive: true);
      }
      if (patValue.trim().isNotEmpty) azurePat = patValue.trim();

      final sprintValue = textValue(
        message:
            'Email de review da sprint (opcional; Enter mantém o valor atual)',
        initialValue: reviewerSprint,
        validate: validateOptionalEmail,
      );
      if (sprintValue.cancelled) {
        return ConfigInitialization.cancelled(paths: paths, interactive: true);
      }
      reviewerSprint = sprintValue.value.trim().isEmpty
          ? reviewerSprint
          : sprintValue.value.trim();

      final devValue = textValue(
        message:
            'Email de review de dev (opcional; Enter mantém o valor atual)',
        initialValue: reviewerDev,
        validate: validateOptionalEmail,
      );
      if (devValue.cancelled) {
        return ConfigInitialization.cancelled(paths: paths, interactive: true);
      }
      reviewerDev = devValue.value.trim().isEmpty
          ? reviewerDev
          : devValue.value.trim();

      final assignedValue = textValue(
        message: 'Email de review/responsável do card de teste (opcional; usado em System.AssignedTo)',
        initialValue: testAssignedTo,
        validate: validateOptionalEmail,
      );
      if (assignedValue.cancelled) {
        return ConfigInitialization.cancelled(paths: paths, interactive: true);
      }
      testAssignedTo = assignedValue.value.trim().isEmpty
          ? testAssignedTo
          : assignedValue.value.trim();

      final selectedProvider = prompts.select(
        message: 'Provider padrão',
        options: const [
          PromptOption(value: 'codex', label: 'Codex local'),
          PromptOption(value: 'opencode', label: 'OpenCode local'),
          PromptOption(value: 'openai-compatible', label: 'OpenAI-compatible'),
        ],
        initialValue: provider,
      );
      if (selectedProvider == null) {
        return ConfigInitialization.cancelled(paths: paths, interactive: true);
      }
      provider = await use.unwrap(_providerResult(selectedProvider));

      if (provider == 'codex') {
        final modelValue = textValue(
          message: 'Modelo do Codex',
          initialValue: codexModel,
          placeholder: codexModelDefault,
        );
        if (modelValue.cancelled) {
          return ConfigInitialization.cancelled(
            paths: paths,
            interactive: true,
          );
        }
        codexModel = modelValue.value.trim().isEmpty
            ? codexModelDefault
            : modelValue.value.trim();
        final reasoning = prompts.select(
          message: 'Thinking level do Codex',
          options: _reasoningOptions,
          initialValue: codexThinking,
        );
        if (reasoning == null) {
          return ConfigInitialization.cancelled(
            paths: paths,
            interactive: true,
          );
        }
        codexThinking = await use.unwrap(
          _reasoningResult(reasoning, codexThinking),
        );
      }
      if (provider == 'opencode') {
        final modelValue = textValue(
          message: 'Modelo do OpenCode',
          initialValue: opencodeModel,
          placeholder: opencodeModelDefault,
        );
        if (modelValue.cancelled) {
          return ConfigInitialization.cancelled(
            paths: paths,
            interactive: true,
          );
        }
        opencodeModel = modelValue.value.trim().isEmpty
            ? opencodeModelDefault
            : modelValue.value.trim();
        final reasoning = prompts.select(
          message: 'Thinking level do OpenCode',
          options: _reasoningOptions,
          initialValue: opencodeThinking,
        );
        if (reasoning == null) {
          return ConfigInitialization.cancelled(
            paths: paths,
            interactive: true,
          );
        }
        opencodeThinking = await use.unwrap(
          _reasoningResult(reasoning, opencodeThinking),
        );
      }
      if (provider == 'openai-compatible') {
        final urlValue = textValue(
          message: 'Base URL OpenAI-compatible',
          initialValue: baseUrl,
          placeholder: defaultBaseUrl,
        );
        if (urlValue.cancelled) {
          return ConfigInitialization.cancelled(
            paths: paths,
            interactive: true,
          );
        }
        baseUrl = urlValue.value.trim().isEmpty
            ? defaultBaseUrl
            : urlValue.value.trim();
        final modelValue = textValue(
          message: 'Modelo OpenAI-compatible',
          initialValue: compatibleModel,
          placeholder: defaultCompatibleModel,
        );
        if (modelValue.cancelled) {
          return ConfigInitialization.cancelled(
            paths: paths,
            interactive: true,
          );
        }
        compatibleModel = modelValue.value.trim().isEmpty
            ? defaultCompatibleModel
            : modelValue.value.trim();
        final reasoning = prompts.select(
          message: 'Thinking level OpenAI-compatible',
          options: _reasoningOptions,
          initialValue: compatibleReasoning,
        );
        if (reasoning == null) {
          return ConfigInitialization.cancelled(
            paths: paths,
            interactive: true,
          );
        }
        compatibleReasoning = await use.unwrap(
          _reasoningResult(reasoning, compatibleReasoning),
        );
        final keyValue = prompts.password(
          message: 'API key (Enter para manter a atual ou deixar vazia)',
        );
        if (keyValue == null) {
          return ConfigInitialization.cancelled(
            paths: paths,
            interactive: true,
          );
        }
        if (keyValue.trim().isNotEmpty) apiKey = keyValue.trim();
      }

      final areaValue = textValue(
        message: 'AreaPath padrão para Test Cases (opcional)',
        initialValue: testAreaPath,
      );
      if (areaValue.cancelled) {
        return ConfigInitialization.cancelled(paths: paths, interactive: true);
      }
      testAreaPath = areaValue.value.trim().isEmpty
          ? testAreaPath
          : areaValue.value;

      final teamValue = textValue(
        message: 'Team padrão para Test Cases',
        initialValue: testTeam,
      );
      if (teamValue.cancelled) {
        return ConfigInitialization.cancelled(paths: paths, interactive: true);
      }
      testTeam = teamValue.value.trim().isEmpty
          ? 'DevOps'
          : teamValue.value.trim();

      final programValue = textValue(
        message: 'Program padrão para Test Cases',
        initialValue: testProgram,
      );
      if (programValue.cancelled) {
        return ConfigInitialization.cancelled(paths: paths, interactive: true);
      }
      testProgram = programValue.value.trim().isEmpty
          ? 'Agrotrace'
          : programValue.value.trim();
    }

    final configJson = JsonEncoder.withIndent('  ').convert({
      'providers': [provider],
      'baseUrl': baseUrl,
      'compatibleModel': compatibleModel,
      'compatibleReasoning': compatibleReasoning,
      'codexModel': codexModel,
      'codexReasoning': codexThinking,
      'opencodeModel': opencodeModel,
      'opencodeReasoning': opencodeThinking,
      'reviewerDev': reviewerDev,
      'reviewerSprint': reviewerSprint,
      'testAreaPath': testAreaPath,
      'testAssignedTo': testAssignedTo,
      'testTeam': testTeam,
      'testProgram': testProgram,
      'apiKey': apiKey,
    });
    await use.unwrap(
      files.write(paths.configFile, '$configJson\n', mode: 0x180),
    );

    final dotEnvValues = <String, String>{};
    if (azurePat.isNotEmpty) dotEnvValues['AZURE_PAT'] = azurePat;
    if (runtime.interactive || reviewerDev.isNotEmpty) {
      dotEnvValues['PR_REVIEWER_DEV'] = reviewerDev;
    }
    if (runtime.interactive || reviewerSprint.isNotEmpty) {
      dotEnvValues['PR_REVIEWER_SPRINT'] = reviewerSprint;
    }
    if (runtime.interactive || testAssignedTo.isNotEmpty) {
      dotEnvValues['TEST_CARD_ASSIGNED_TO'] = testAssignedTo;
    }
    if (dotEnvValues.isNotEmpty) {
      final envFileExists = await use.unwrap(files.exists(paths.envFile));
      final current = envFileExists
          ? await use.unwrap(files.read(paths.envFile))
          : '';
      await use.unwrap(
        files.write(
          paths.envFile,
          '${_writeDotEnvValues(current, dotEnvValues)}\n',
          mode: 0x180,
        ),
      );
    }
    final templateExists = await use.unwrap(files.exists(paths.templateFile));
    if (!templateExists) {
      await use.unwrap(
        files.write(paths.templateFile, '$defaultTemplate\n', mode: 0x180),
      );
    }
    return ConfigInitialization(
      paths: paths,
      interactive: runtime.interactive,
      azurePatConfigured: azurePat.isNotEmpty,
    );
  });
}

const codexModelDefault = codexModel;
const codexReasoningDefault = codexReasoning;
const opencodeModelDefault = opencodeModel;
const opencodeReasoningDefault = opencodeReasoning;
const compatibleReasoningDefault = compatibleReasoning;

const _reasoningOptions = [
  PromptOption(
    value: 'provider-default',
    label: 'Padrão do provider',
    hint: 'usa a configuração nativa',
  ),
  PromptOption(value: 'none', label: 'None', hint: 'sem reasoning adicional'),
  PromptOption(
    value: 'minimal',
    label: 'Minimal',
    hint: 'resposta mais rápida',
  ),
  PromptOption(value: 'low', label: 'Low', hint: 'reasoning leve'),
  PromptOption(
    value: 'medium',
    label: 'Medium',
    hint: 'equilíbrio entre custo e profundidade',
  ),
  PromptOption(value: 'high', label: 'High', hint: 'reasoning aprofundado'),
  PromptOption(
    value: 'xhigh',
    label: 'XHigh',
    hint: 'máxima profundidade quando suportado',
  ),
];

AppEffect<Config> _loadConfig(
  CliOptions options,
  Map<String, String> environment,
  Map<String, String> dotEnv,
  Map<String, Object?> json,
  String template,
) => Effect.tryAsync(() {
  final defaultProviders = <ProviderName>[
    'codex',
    'opencode',
    'openai-compatible',
  ];
  final providerEnvironment = environment['PR_AI_PROVIDERS']?.isNotEmpty == true
      ? environment['PR_AI_PROVIDERS']
      : environment['PR_PROVIDERS']?.isNotEmpty == true
      ? environment['PR_PROVIDERS']
      : dotEnv['PR_AI_PROVIDERS']?.isNotEmpty == true
      ? dotEnv['PR_AI_PROVIDERS']
      : dotEnv['PR_PROVIDERS']?.isNotEmpty == true
      ? dotEnv['PR_PROVIDERS']
      : null;
  final providers = options.provider != null
      ? [parseProvider(options.provider!)]
      : providerEnvironment != null
      ? parseProviderList(providerEnvironment)
      : json['providers'] is List && (json['providers'] as List).isNotEmpty
      ? (json['providers'] as List)
            .map((value) => parseProvider(value is String ? value : '$value'))
            .toList()
      : defaultProviders;

  final compatibleReasoningValue = _firstNonNull(<Object?>[
    environment['PR_AI_REASONING'],
    environment['PR_AI_THINKING_LEVEL'],
    dotEnv['PR_AI_REASONING'],
    dotEnv['PR_AI_THINKING_LEVEL'],
    json['compatibleReasoning'],
  ]);
  final codexReasoningValue = _firstNonNull(<Object?>[
    environment['PR_CODEX_REASONING'],
    environment['PR_CODEX_THINKING_LEVEL'],
    dotEnv['PR_CODEX_REASONING'],
    dotEnv['PR_CODEX_THINKING_LEVEL'],
    json['codexReasoning'],
  ]);
  final opencodeReasoningValue = _firstNonNull(<Object?>[
    environment['PR_OPENCODE_REASONING'],
    environment['PR_OPENCODE_THINKING_LEVEL'],
    dotEnv['PR_OPENCODE_REASONING'],
    dotEnv['PR_OPENCODE_THINKING_LEVEL'],
    json['opencodeReasoning'],
  ]);
  return Config(
    providers: providers,
    baseUrl:
        options.baseUrl ??
        environment['PR_AI_BASE_URL'] ??
        dotEnv['PR_AI_BASE_URL'] ??
        _stringValue(json['baseUrl']) ??
        defaultBaseUrl,
    compatibleModel:
        options.model ??
        environment['PR_AI_MODEL'] ??
        dotEnv['PR_AI_MODEL'] ??
        _stringValue(json['compatibleModel']) ??
        defaultCompatibleModel,
    compatibleReasoning: parseReasoningLevel(
      compatibleReasoningValue,
      compatibleReasoning,
    ),
    codexModel:
        options.model ??
        environment['PR_CODEX_MODEL'] ??
        dotEnv['PR_CODEX_MODEL'] ??
        _stringValue(json['codexModel']) ??
        codexModelDefault,
    codexReasoning: parseReasoningLevel(
      codexReasoningValue,
      codexReasoningDefault,
    ),
    opencodeModel:
        options.model ??
        environment['PR_OPENCODE_MODEL'] ??
        dotEnv['PR_OPENCODE_MODEL'] ??
        _stringValue(json['opencodeModel']) ??
        opencodeModelDefault,
    opencodeReasoning: parseReasoningLevel(
      opencodeReasoningValue,
      opencodeReasoningDefault,
    ),
    azurePat:
        environment['AZURE_DEVOPS_PAT'] ??
        environment['AZURE_PAT'] ??
        dotEnv['AZURE_DEVOPS_PAT'] ??
        dotEnv['AZURE_PAT'] ??
        _stringValue(json['azurePat']) ??
        '',
    reviewerDev:
        environment['PR_REVIEWER_DEV'] ??
        dotEnv['PR_REVIEWER_DEV'] ??
        _stringValue(json['reviewerDev']) ??
        '',
    reviewerSprint:
        environment['PR_REVIEWER_SPRINT'] ??
        dotEnv['PR_REVIEWER_SPRINT'] ??
        _stringValue(json['reviewerSprint']) ??
        '',
    testAreaPath:
        environment['TEST_CARD_AREA_PATH'] ??
        dotEnv['TEST_CARD_AREA_PATH'] ??
        _stringValue(json['testAreaPath']) ??
        '',
    testAssignedTo:
        environment['TEST_CARD_ASSIGNED_TO'] ??
        dotEnv['TEST_CARD_ASSIGNED_TO'] ??
        _stringValue(json['testAssignedTo']) ??
        '',
    testTeam:
        environment['TEST_CARD_TEAM'] ??
        dotEnv['TEST_CARD_TEAM'] ??
        _stringValue(json['testTeam']) ??
        'DevOps',
    testProgram:
        environment['TEST_CARD_PROGRAM'] ??
        dotEnv['TEST_CARD_PROGRAM'] ??
        _stringValue(json['testProgram']) ??
        'Agrotrace',
    apiKey:
        options.apiKey ??
        environment['PR_AI_API_KEY'] ??
        environment['OPENAI_API_KEY'] ??
        dotEnv['PR_AI_API_KEY'] ??
        dotEnv['OPENAI_API_KEY'] ??
        _stringValue(json['apiKey']) ??
        '',
    template: template,
  );
}, onError: (error, _) => CliFailure(error.toString()));

AppEffect<Map<String, Object?>> _readJson(
  String contents,
  String path,
) => Effect.tryAsync(() {
  final value = jsonDecode(contents);
  if (value is! Map) {
    throw const FormatException('o valor raiz não é um objeto');
  }
  return value.map<String, Object?>((key, value) => MapEntry('$key', value));
}, onError: (error, _) => CliFailure('Configuração inválida em $path: $error'));

Map<String, String> _parseDotEnv(String contents) {
  final values = <String, String>{};
  for (final line in contents.split('\n')) {
    final match = RegExp(r'^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)\s*$')
        .firstMatch(line);
    if (match == null) continue;
    final key = match.group(1);
    final raw = match.group(2);
    if (key == null || raw == null) continue;
    var value = raw;
    if (RegExp(r'''^['"]''').hasMatch(value)) {
      value = value.substring(1);
    }
    if (RegExp(r'''['"]$''').hasMatch(value)) {
      value = value.substring(0, value.length - 1);
    }
    values[key] = value;
  }
  return values;
}

String _writeDotEnvValues(String contents, Map<String, String> values) {
  final lines = contents.isEmpty ? <String>[] : contents.split('\n');
  for (final entry in values.entries) {
    final escaped = entry.value.replaceAll(r'\', r'\\').replaceAll('"', r'\"');
    final line = '${entry.key}="$escaped"';
    final pattern = RegExp('^\\s*${RegExp.escape(entry.key)}\\s*=');
    final index = lines.indexWhere(pattern.hasMatch);
    if (index >= 0) {
      lines[index] = line;
    } else {
      lines.add(line);
    }
  }
  return lines.join('\n').replaceFirst(RegExp(r'\n+$'), '');
}

String? _stringValue(Object? value) => value is String ? value : null;

Object? _firstNonNull(Iterable<Object?> values) {
  for (final value in values) {
    if (value != null) return value;
  }
  return null;
}

AppEffect<String> _existingProvider(Map<String, Object?> existing) =>
    Effect.tryAsync(() {
      final providers = existing['providers'];
      if (providers is! List || providers.isEmpty) {
        return 'codex';
      }
      final value = providers.first;
      return parseProvider(value is String ? value : '$value');
    }, onError: (error, _) => CliFailure(error.toString()));

AppEffect<ReasoningLevel> _reasoningResult(
  Object? value,
  ReasoningLevel fallback,
) => Effect.tryAsync(
  () => parseReasoningLevel(value, fallback),
  onError: (error, _) => CliFailure(error.toString()),
);

AppEffect<ProviderName> _providerResult(String value) => Effect.tryAsync(
  () => parseProvider(value),
  onError: (error, _) => CliFailure(error.toString()),
);

CliFailure _fileSystemFailure(Exception error) {
  final message = error is FileSystemException ? error.message : '$error';
  return CliFailure(message);
}

String _joinPath(String parent, String child) {
  if (parent.isEmpty) return child;
  if (parent.endsWith('/') || parent.endsWith('\\')) return '$parent$child';
  return '$parent${Platform.pathSeparator}$child';
}
