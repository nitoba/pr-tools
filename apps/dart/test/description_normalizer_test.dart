import 'dart:convert';

import 'package:better_effect/better_effect.dart';
import 'package:pr_tools/src/application/ai/description_models.dart';
import 'package:pr_tools/src/application/ai/description_normalizer.dart';
import 'package:test/test.dart';

void main() {
  test('prefers structured output and applies cleaning rules', () async {
    final result = await _normalize(
      {'title': '"Ajusta login"', 'body': '\n---\n## Descrição\n'},
      'ignored',
      'feature/1-login',
    );

    expect(
      result,
      const PrDescription(title: 'Ajusta login', body: '## Descrição'),
    );
  });

  test('parses fenced JSON after removing think blocks', () async {
    final result = await _normalize(
      null,
      '<think>raciocínio</think>\n```json\n{"title":"Adiciona filtro","body":"## Descrição\\nInclui CPF."}\n```',
      'feature/1-filtro',
    );

    expect(result.title, 'Adiciona filtro');
    expect(result.body, '## Descrição\nInclui CPF.');
  });

  test('extracts JSON before trailing provider context', () async {
    final result = await _normalize(null, '''Resposta final:
{"title":"Ajusta login","body":"## Descrição\\nCorrige fluxo."}

## Contexto Git
**Branch:** feature/1-login
### Git Diff
diff --git a/file b/file
''', 'feature/1-login');

    expect(
      result,
      const PrDescription(
        title: 'Ajusta login',
        body: '## Descrição\nCorrige fluxo.',
      ),
    );
  });

  test('decodes a JSON string returned by a provider', () async {
    final encoded = jsonEncode({
      'title': 'Ajusta login',
      'body': '## Checklist de testes\n- Executar o fluxo.',
    });

    final result = await _normalize(encoded, 'ignored', 'feature/1-login');

    expect(result.title, 'Ajusta login');
    expect(result.body, '## Checklist de testes\n- Executar o fluxo.');
  });

  test('converts literal escaped line breaks in a structured body', () async {
    final encoded = jsonEncode({
      'title': 'Ajusta login',
      'body': r'## Checklist de testes\n- Executar o fluxo.',
    });

    final result = await _normalize(null, encoded, 'feature/1-login');

    expect(result.body, '## Checklist de testes\n- Executar o fluxo.');
  });

  test('keeps ordinary backslash sequences in the body', () async {
    final result = await _normalize(
      {'title': 'Documenta caminho', 'body': r'C:\repo\new'},
      'ignored',
      'feature/1',
    );

    expect(result.body, r'C:\repo\new');
  });

  test('removes echoed git context from the body', () async {
    final result = await _normalize(
      {
        'title': 'Ajusta login',
        'body': '## Descrição\nCorrige fluxo.\n\n## Contexto Git\n**Branch:** feature/1',
      },
      'ignored',
      'feature/1-login',
    );

    expect(result.body, '## Descrição\nCorrige fluxo.');
  });

  test('keeps the legacy title marker out of the body', () async {
    final result = await _normalize(
      null,
      'TÍTULO: Ajusta login\n\n## Descrição\nCorrige fluxo.',
      'feature/1-login',
    );

    expect(
      result,
      const PrDescription(
        title: 'Ajusta login',
        body: '## Descrição\nCorrige fluxo.',
      ),
    );
  });

  test('falls back to branch and defaults for empty text', () async {
    final result = await _normalize(null, '', 'feature/42-login');

    expect(
      result,
      const PrDescription(
        title: 'feature/42-login',
        body: 'Sem descrição gerada.',
      ),
    );
  });

  test('truncates a fallback title to 80 characters', () async {
    final result = await _normalize(null, 'a' * 81, 'feature/1');

    expect(result.title.length, 80);
    expect(result.body, 'a' * 81);
  });
}

Future<PrDescription> _normalize(
  Object? output,
  String text,
  String branch,
) async {
  final result = await Module([])
      .run(normalizeDescription(output, text, branch));
  return result.fold((value) => value, (failure) => throw failure);
}
