import 'package:better_effect/better_effect.dart';
import 'package:genkit/genkit.dart';
import 'package:genkit_openai/genkit_openai.dart';

import '../../application/ai/description_generator.dart';
import '../../application/ai/description_models.dart';
import '../../application/ai/description_normalizer.dart';
import '../../app/app_effect.dart';
import '../../application/config/config_models.dart';

const _pluginName = 'pr-tools-openai-compatible';

final class GenkitCompatibleDescriptionGenerator
    implements CompatibleDescriptionGenerator {
  const GenkitCompatibleDescriptionGenerator();

  @override
  AppEffect<PrDescription> generate({
    required Config config,
    required String system,
    required String prompt,
    required String branch,
  }) => _generateWithFallback(config, system, prompt, branch);

  AppEffect<PrDescription> _generateWithFallback(
    Config config,
    String system,
    String prompt,
    String branch,
  ) => .result((use) async {
    final structured = await use.unwrap(
      _generateOnce(
        config: config,
        system: system,
        prompt: prompt,
        branch: branch,
        structured: true,
      ).either(),
    );
    PrDescription? description;
    structured.fold<void>((value) => description = value, (_) {});
    if (description != null) return description!;
    return use.unwrap(
      _generateOnce(
        config: config,
        system: system,
        prompt: prompt,
        branch: branch,
        structured: false,
      ),
    );
  });

  AppEffect<PrDescription> _generateOnce({
    required Config config,
    required String system,
    required String prompt,
    required String branch,
    required bool structured,
  }) => Effect.result((use) async {
    final ai = await use.acquire(
      Effect.tryAsync(
        () => Genkit(
          plugins: [
            openAI(
              name: _pluginName,
              apiKey: config.apiKey.trim().isEmpty ? 'unused' : config.apiKey,
              baseUrl: _normalizeBaseUrl(config.baseUrl),
              models: [CustomModelDefinition(name: config.compatibleModel)],
            ),
          ],
          promptDir: null,
        ),
        onError: (error, _) => AiFailure('openai-compatible falhou: $error'),
      ),
      release: (ai, _) => ai.shutdown(),
    );
    final response = await use.unwrap(
      Effect.tryAsync(
        () => ai.generate(
          model: openAI.model(config.compatibleModel, namespace: _pluginName),
          system: _systemPrompt(config, system),
          prompt: prompt,
          config: OpenAIChatOptions(jsonMode: structured),
          outputFormat: structured ? 'json' : null,
          outputInstructions: 'Responda com JSON contendo title e body.',
        ),
        onError: (error, _) => AiFailure('openai-compatible falhou: $error'),
      ),
    );
    final output = response.output ?? response.jsonOutput;
    if (response.text.trim().isEmpty && output == null) {
      use.fail(const AiFailure('openai-compatible não retornou texto.'));
    }
    return use.unwrap(normalizeDescription(output, response.text, branch));
  });

  String _normalizeBaseUrl(String value) {
    final withoutTrailingSlash = value.endsWith('/')
        ? value.substring(0, value.length - 1)
        : value;
    const suffix = '/chat/completions';
    return withoutTrailingSlash.endsWith(suffix)
        ? withoutTrailingSlash.substring(
            0,
            withoutTrailingSlash.length - suffix.length,
          )
        : withoutTrailingSlash;
  }

  String _systemPrompt(Config config, String system) {
    if (config.compatibleReasoning == 'provider-default') return system;
    return '$system\n\nUse nível de reasoning ${config.compatibleReasoning} antes de responder.';
  }
}
