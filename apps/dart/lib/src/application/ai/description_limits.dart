import 'package:result_dart/result_dart.dart';

import '../../app/app_failure.dart';
import 'description_models.dart';

/// Azure DevOps accepts PR descriptions with fewer than 4000 characters.
const azurePrDescriptionMaxLength = 4000;

const azurePrDescriptionPromptRules = '''
REGRAS OBRIGATÓRIAS DO CAMPO "body":
- O body deve ter menos de 4000 caracteres (limite estrito: no máximo 3999).
- Conte todos os caracteres do Markdown, incluindo espaços e quebras de linha.
- Preserve somente informações sustentadas pelo contexto; seja conciso e priorize o que mudou e por quê.
- Nunca ultrapasse esse limite, não inclua o contexto Git na resposta e não escreva texto fora do JSON.''';

final class DescriptionLengthFailure extends AppFailure {
  const DescriptionLengthFailure(this.length)
    : super(
        'A descrição do PR excede o limite do Azure DevOps: $length '
        'caracteres (máximo ${azurePrDescriptionMaxLength - 1}).',
        1,
      );

  final int length;
}

bool isAzurePrDescriptionWithinLimit(String body) =>
    body.length < azurePrDescriptionMaxLength;

ResultDart<PrDescription, DescriptionLengthFailure> validateAzurePrDescription(
  PrDescription description,
) {
  final length = description.body.length;
  if (!isAzurePrDescriptionWithinLimit(description.body)) {
    return Failure<PrDescription, DescriptionLengthFailure>(
      DescriptionLengthFailure(length),
    );
  }
  return Success<PrDescription, DescriptionLengthFailure>(description);
}
