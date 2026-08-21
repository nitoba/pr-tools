import 'dart:io';

import 'package:better_effect/better_effect.dart';

import '../../application/ai/description_generator.dart';
import '../../application/ai/description_models.dart';
import '../../application/ai/description_normalizer.dart';
import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import '../../application/config/config_models.dart';
import '../../application/process/process_runner.dart';

final class DescriptionGeneratorLive implements DescriptionGenerator {
  const DescriptionGeneratorLive();

  @override
  AppEffect<GeneratedDescription> generate({
    required Config config,
    required String system,
    required String prompt,
    required String branch,
    DescriptionReporter? report,
  }) => .result((use) async {
    final errors = <String>[];
    for (final provider in config.providers) {
      final model = _modelFor(config, provider);
      report?.call(provider, model);
      final attempt = await use.unwrap(
        _attempt(
          provider: provider,
          config: config,
          system: system,
          prompt: prompt,
          branch: branch,
        ).either(),
      );
      final generated = attempt.fold<GeneratedDescription?>((value) => value, (
        failure,
      ) {
        errors.add('$provider: ${failure.message}');
        return null;
      });
      if (generated != null) return generated;
    }
    use.fail(
      AiFailure(
        'Todos os providers falharam:\n${errors.map((error) => '  $error').join('\n')}',
      ),
    );
  });

  AppEffect<GeneratedDescription> _attempt({
    required String provider,
    required Config config,
    required String system,
    required String prompt,
    required String branch,
  }) => .result((use) async {
    final description = switch (provider) {
      'codex' => await use.unwrap(_runCodex(config, system, prompt, branch)),
      'opencode' => await use.unwrap(
        _runOpenCode(config, system, prompt, branch),
      ),
      _ => await use.unwrap(
        use<CompatibleDescriptionGenerator>().generate(
          config: config,
          system: system,
          prompt: prompt,
          branch: branch,
        ),
      ),
    };
    return GeneratedDescription(
      description: description,
      provider: provider,
      model: _modelFor(config, provider),
    );
  });

  AppEffect<PrDescription> _runCodex(
    Config config,
    String system,
    String prompt,
    String branch,
  ) => .result((use) async {
    final args = <String>[
      'exec',
      '-m',
      config.codexModel,
      '-c',
      'approval_policy=never',
      '-c',
      'sandbox_mode=read-only',
      '--skip-git-repo-check',
      '--color',
      'never',
    ];
    if (config.codexReasoning != 'provider-default') {
      args.addAll(['-c', 'model_reasoning_effort=${config.codexReasoning}']);
    }
    args.add('$system\n\n$prompt');
    final result = await _runProcess(use, 'codex', args);
    _validateProcess(result, 'codex', use);
    return use.unwrap(normalizeDescription(null, result.stdout, branch));
  });

  AppEffect<PrDescription> _runOpenCode(
    Config config,
    String system,
    String prompt,
    String branch,
  ) => .result((use) async {
    final path =
        '${Directory.systemTemp.path}/prt-opencode-${DateTime.now().microsecondsSinceEpoch}.md';
    final promptPath = await use.acquire(
      _writePrompt(path, '$system\n\n$prompt'),
      release: (value, _) => use.tryAsync(() async {
        await File(value).delete();
        return unit;
      }, onError: (error, _) => AiFailure('opencode falhou: $error')),
    );
    final args = <String>[
      'run',
      '--format',
      'default',
      '--pure',
      '--agent',
      'general',
      '--model',
      config.opencodeModel,
      '--file',
      promptPath,
      'Gere o JSON solicitado usando o arquivo anexado. Não execute ferramentas.',
    ];
    if (config.opencodeReasoning != 'provider-default') {
      args.addAll(['--variant', config.opencodeReasoning]);
    }
    final result = await _runProcess(use, 'opencode', args);
    _validateProcess(result, 'opencode', use);
    return use.unwrap(normalizeDescription(null, result.stdout, branch));
  });

  Future<ProcessResult> _runProcess(
    EffectContext<AppFailure> use,
    String provider,
    List<String> args,
  ) async {
    final processes = use<ProcessRunner>();
    return await use.unwrap(
      processes
          .run(provider, args)
          .mapError((error) => AiFailure('$provider falhou: ${error.message}')),
    );
  }

  AppEffect<String> _writePrompt(String path, String prompt) =>
      Effect.tryAsync(() async {
        await File(path).writeAsString(prompt);
        return path;
      }, onError: (error, _) => AiFailure('opencode falhou: $error'));

  void _validateProcess(
    ProcessResult result,
    String provider,
    EffectContext<AppFailure> use,
  ) {
    if (result.error != null || result.exitCode != 0) {
      use.fail(
        AiFailure(
          '$provider falhou: ${result.error ?? (result.stderr.isNotEmpty ? result.stderr : 'código ${result.exitCode}')}',
        ),
      );
    }
    if (result.stdout.trim().isEmpty) {
      use.fail(AiFailure('$provider não retornou texto.'));
    }
  }

  String _modelFor(Config config, String provider) {
    if (provider == 'codex') return config.codexModel;
    if (provider == 'opencode') return config.opencodeModel;
    return config.compatibleModel;
  }
}
