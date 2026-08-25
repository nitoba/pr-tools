import 'dart:convert';

import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import 'description_models.dart';

AppEffect<PrDescription> normalizeDescription(
  Object? output,
  String text,
  String branch,
) => Effect.result((use) async {
  final direct = _descriptionFrom(output);
  if (direct != null) return cleanDescription(direct);

  final cleanText = _stripThinkBlocks(text);
  final jsonText = cleanText
      .replaceFirst(RegExp(r'^```(?:json)?\s*', caseSensitive: false), '')
      .replaceFirst(RegExp(r'\s*```$'), '');
  final json = await use.unwrap(_decodeJson(jsonText).either());
  final decoded = json.fold(_descriptionFrom, (_) => null);
  if (decoded != null) {
    return cleanDescription(decoded);
  }

  final titleMatch = RegExp(
    r'^\s*T[IÍ]TULO\s*:\s*(.+)$',
    caseSensitive: false,
    multiLine: true,
  ).firstMatch(cleanText);
  final titleFromMarker = titleMatch?.group(1) ?? '';
  final firstLine = cleanText
      .split('\n')
      .where((line) => line.isNotEmpty)
      .firstOrNull;
  final title = titleFromMarker.trim().isNotEmpty
      ? titleFromMarker.trim()
      : firstLine == null
      ? branch
      : _slice(firstLine, 80);
  final markerText = titleMatch?.group(0);
  final body = markerText == null
      ? cleanText
      : cleanText
            .substring(cleanText.indexOf(markerText) + markerText.length)
            .trim();
  return cleanDescription(PrDescription(title: title, body: body));
});

AppEffect<Object> _decodeJson(String value) => Effect.tryAsync(
  () => jsonDecode(value) ?? const _JsonNull(),
  onError: (error, _) => CliFailure('Descrição IA inválida: $error'),
);

final class _JsonNull {
  const _JsonNull();
}

PrDescription cleanDescription(PrDescription description) {
  final title = _slice(
    description.title.replaceAll(RegExp(r'''^['"]|['"]$'''), '').trim(),
    80,
  );
  final body = description.body.replaceFirst(RegExp(r'^\s*---\s*'), '').trim();
  return PrDescription(
    title: title.isEmpty ? 'Atualiza código' : title,
    body: body.isEmpty ? 'Sem descrição gerada.' : body,
  );
}

PrDescription? _descriptionFrom(Object? value) {
  if (value is PrDescription) return value;
  if (value is! Map) return null;
  final title = value['title'];
  final body = value['body'];
  if (title is! String || body is! String) return null;
  return PrDescription(title: title, body: body);
}

String _stripThinkBlocks(String value) {
  return value
      .replaceAll(
        RegExp(r'<think>.*?</think>', caseSensitive: false, dotAll: true),
        '',
      )
      .replaceAll(RegExp(r'</?think>', caseSensitive: false), '')
      .trim();
}

String _slice(String value, int length) {
  return value.length > length ? value.substring(0, length) : value;
}
