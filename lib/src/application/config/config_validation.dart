import 'config_models.dart';

const providerValues = <ProviderName>{'codex', 'opencode', 'openai-compatible'};

const reasoningLevelValues = <ReasoningLevel>{
  'provider-default',
  'none',
  'minimal',
  'low',
  'medium',
  'high',
  'xhigh',
};

ProviderName parseProvider(String value) {
  if (providerValues.contains(value)) return value;
  throw FormatException(
    'Provider inválido: $value. Use codex, opencode ou openai-compatible.',
  );
}

List<ProviderName> parseProviderList(String value) {
  final providers = value
      .split(',')
      .map((item) => item.trim())
      .where((item) => item.isNotEmpty)
      .map(parseProvider)
      .toList();
  if (providers.isEmpty) {
    throw const FormatException('Configure pelo menos um provider.');
  }
  return providers;
}

ReasoningLevel parseReasoningLevel(Object? value, ReasoningLevel fallback) {
  if (value == null) return fallback;
  if (value is String && value.trim().isEmpty) return fallback;
  if (value is String && reasoningLevelValues.contains(value)) return value;
  throw FormatException(
    'Nível de thinking inválido (${value.runtimeType}). Use provider-default, '
    'none, minimal, low, medium, high ou xhigh.',
  );
}

String? validateOptionalEmail(String? value) {
  final normalized = (value ?? '').trim();
  if (normalized.isEmpty) return null;
  final valid = RegExp(r'^[^\s@]+@[^\s@]+\.[^\s@]+$').hasMatch(normalized);
  return valid ? null : 'Informe um email válido ou deixe vazio.';
}
