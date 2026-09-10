import 'package:better_effect/better_effect.dart';

import '../../application/ai/description_generator.dart';
import '../../application/ai/description_rewriter.dart';
import '../../application/ai/description_models.dart';
import '../../application/config/config_models.dart';
import '../../app/app_effect.dart';

const _rewriteInstructions = '''Você é um revisor técnico responsável por reduzir uma descrição de Pull Request para publicação no Azure DevOps.

Reescreva a descrição recebida preservando o sentido, os fatos, as decisões e as informações importantes. Remova redundâncias, detalhes secundários e palavras desnecessárias; não invente informações e não trunque frases no meio.

Retorne um objeto JSON com exatamente estes campos:
- "title": o título original, corrigido apenas se necessário.
- "body": a descrição em Markdown, com menos de 4000 caracteres (no máximo 3999).

Conte todos os caracteres do Markdown, incluindo espaços e quebras de linha. Responda somente com o objeto JSON, sem explicações, contexto Git ou texto adicional.''';

final class DescriptionRewriterLive implements DescriptionRewriter {
  const DescriptionRewriterLive();

  @override
  AppEffect<GeneratedDescription> rewrite({
    required Config config,
    required String system,
    required String branch,
    required PrDescription description,
    DescriptionReporter? report,
  }) => Effect.result((use) async {
    final generator = use<DescriptionGenerator>();
    return use.unwrap(
      generator.generate(
        config: config,
        system: '$system\n\n$_rewriteInstructions',
        prompt: _buildRewritePrompt(description),
        branch: branch,
        report: report,
      ),
    );
  });

  String _buildRewritePrompt(PrDescription description) {
    const fence = '\u0060\u0060\u0060';
    return '''## Descrição original gerada

### Título
${description.title}

### Body
$fence\n${description.body}\n$fence

Reescreva o objeto acima seguindo rigorosamente as instruções de redução.''';
  }
}
