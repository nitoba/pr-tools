import 'package:better_effect/better_effect.dart';
import 'package:pr_tools/src/application/ai/description_generator.dart';
import 'package:pr_tools/src/application/ai/description_models.dart';
import 'package:pr_tools/src/application/ai/description_rewriter.dart';
import 'package:pr_tools/src/application/config/config_models.dart';
import 'package:pr_tools/src/app/app_effect.dart';
import 'package:pr_tools/src/infrastructure/ai/description_rewriter.dart';
import 'package:test/test.dart';

void main() {
  test('sends the complete oversized description to a rewrite agent', () async {
    final generator = _RecordingGenerator();
    final original = PrDescription(
      title: 'Título original',
      body: 'conteúdo que precisa ser resumido ' * 150,
    );
    final module = Module([
      .instance<DescriptionGenerator>(generator),
      .provide<DescriptionRewriter>(DescriptionRewriterLive.new),
    ]);

    final result = await module.run(
      Effect.result((use) async {
        return use.unwrap(
          use<DescriptionRewriter>().rewrite(
            config: _config,
            system: 'sistema original',
            branch: 'feature/1',
            description: original,
          ),
        );
      }),
    );

    expect(result.getOrNull()?.description.body, 'body reescrito');
    expect(generator.system, contains('sistema original'));
    expect(generator.system, contains('no máximo 3999'));
    expect(generator.prompt, contains(original.title));
    expect(generator.prompt, contains(original.body));
  });
}

final class _RecordingGenerator implements DescriptionGenerator {
  String? system;
  String? prompt;

  @override
  AppEffect<GeneratedDescription> generate({
    required Config config,
    required String system,
    required String prompt,
    required String branch,
    DescriptionReporter? report,
  }) {
    this.system = system;
    this.prompt = prompt;
    return Effect.succeed(
      const GeneratedDescription(
        description: PrDescription(title: 'Título', body: 'body reescrito'),
        provider: 'rewriter',
        model: 'model',
      ),
    );
  }
}

const _config = Config(
  providers: ['codex'],
  baseUrl: 'https://api.openai.com/v1',
  compatibleModel: 'compatible-model',
  compatibleReasoning: 'provider-default',
  codexModel: 'codex-model',
  codexReasoning: 'high',
  opencodeModel: 'openai/gpt-5.5',
  opencodeReasoning: 'provider-default',
  azurePat: '',
  reviewerDev: '',
  reviewerSprint: '',
  testAreaPath: '',
  testAssignedTo: '',
  testTeam: 'DevOps',
  testProgram: 'Agrotrace',
  apiKey: '',
  template: 'template',
);
