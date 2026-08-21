import 'config_models.dart';

const codexModel = 'gpt-5.6-luna';
const ReasoningLevel codexReasoning = 'high';
const opencodeModel = 'openai/gpt-5.5';
const ReasoningLevel opencodeReasoning = 'provider-default';
const defaultBaseUrl = 'https://api.openai.com/v1';
const defaultCompatibleModel = 'gpt-4o-mini';
const ReasoningLevel compatibleReasoning = 'provider-default';

const defaultTemplate = '''Analise o diff e o log do git fornecidos e gere uma descrição de pull request em português brasileiro.

Retorne um objeto JSON com exatamente estes campos:
- "title": título curto, técnico e descritivo, com no máximo 80 caracteres.
- "body": descrição em Markdown.

O body deve seguir este formato:

## Descrição

Resumo conciso em 1 ou 2 frases do que mudou e por quê.

## Alterações

Liste componentes ou arquivos relevantes e descreva a mudança funcional.

## Tipo de mudança

- [ ] Bug fix
- [ ] Nova feature
- [ ] Breaking change
- [ ] Refactoring

Não invente alterações que não estejam no diff.''';
