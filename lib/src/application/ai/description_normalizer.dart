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
  () => _parseJson(value),
  onError: (error, _) => CliFailure('Descrição IA inválida: $error'),
);

Object _parseJson(String value) {
  try {
    return jsonDecode(value) ?? const _JsonNull();
  } on FormatException {
    final start = value.indexOf('{');
    if (start < 0) rethrow;
    final end = _jsonObjectEnd(value, start);
    if (end == null) rethrow;
    return jsonDecode(value.substring(start, end)) ?? const _JsonNull();
  }
}

int? _jsonObjectEnd(String value, int start) {
  var depth = 0;
  var inString = false;
  var escaped = false;
  for (var index = start; index < value.length; index++) {
    final character = value.codeUnitAt(index);
    if (inString) {
      if (escaped) {
        escaped = false;
      } else if (character == _backslash) {
        escaped = true;
      } else if (character == _quote) {
        inString = false;
      }
      continue;
    }
    if (character == _quote) {
      inString = true;
    } else if (character == _openBrace) {
      depth++;
    } else if (character == _closeBrace && --depth == 0) {
      return index + 1;
    }
  }
  return null;
}

final class _JsonNull {
  const _JsonNull();
}

const _openBrace = 123;
const _closeBrace = 125;
const _quote = 34;
const _backslash = 92;

PrDescription cleanDescription(PrDescription description) {
  final title = _slice(
    description.title.replaceAll(RegExp(r'''^['"]|['"]$'''), '').trim(),
    80,
  );
  final body = _stripGitContext(
    description.body.replaceFirst(RegExp(r'^\s*---\s*'), '').trim(),
  );
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

String _stripGitContext(String value) {
  final marker = RegExp(
    r'^[ \t]*## Contexto Git[ \t]*$',
    multiLine: true,
  ).firstMatch(value);
  return marker == null ? value : value.substring(0, marker.start).trimRight();
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
