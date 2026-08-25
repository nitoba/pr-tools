import 'package:result_dart/result_dart.dart';

import '../../app/app_failure.dart';

final class TestCardValidationFailure extends AppFailure {
  const TestCardValidationFailure(String message) : super(message, 64);
}

String? validatePositiveDecimal(String? value) {
  final text = (value ?? '').trim();
  final number = _decimal(text);
  return number != null && number > 0 ? null : 'Informe um número positivo.';
}

String? validateNonNegativeDecimal(String? value) {
  final text = (value ?? '').trim();
  final number = _decimal(text);
  return number != null && number >= 0 ? null : 'Informe um número válido.';
}

ResultDart<num, TestCardValidationFailure> parsePositiveDecimal(
  String? value,
  num fallback,
  String option,
) {
  if (value == null || value.trim().isEmpty) {
    return Success<num, TestCardValidationFailure>(fallback);
  }
  final number = _decimal(value.trim());
  if (number != null && number > 0) {
    return Success<num, TestCardValidationFailure>(number);
  }
  return Failure<num, TestCardValidationFailure>(
    TestCardValidationFailure('$option deve ser um número positivo.'),
  );
}

ResultDart<int, TestCardValidationFailure> parseExamplesCount(String? value) {
  if (value == null || value.trim().isEmpty) {
    return const Success<int, TestCardValidationFailure>(2);
  }
  final text = value.trim();
  final number = int.tryParse(text);
  if (!RegExp(r'^\d+$').hasMatch(text) || number == null || number > 5) {
    return const Failure<int, TestCardValidationFailure>(
      TestCardValidationFailure('--examples deve ser um número entre 0 e 5.'),
    );
  }
  return Success<int, TestCardValidationFailure>(number);
}

ResultDart<String, TestCardValidationFailure> parseWorkItemId(
  String? value,
  String label,
) {
  final text = value?.trim() ?? '';
  if (!RegExp(r'^\d+$').hasMatch(text)) {
    return Failure<String, TestCardValidationFailure>(
      TestCardValidationFailure('$label inválido: Use um ID numérico.'),
    );
  }
  final number = int.tryParse(text);
  if (number == null || number <= 0) {
    return Failure<String, TestCardValidationFailure>(
      TestCardValidationFailure('$label inválido: Use um ID positivo.'),
    );
  }
  return Success<String, TestCardValidationFailure>(text);
}

String? validateWorkItemId(String? value) {
  if (value == null || value.trim().isEmpty) return null;
  return parseWorkItemId(
    value,
    'Work Item',
  ).fold((_) => null, (error) => error.message);
}

num? _decimal(String value) {
  final number = num.tryParse(value.replaceAll(',', '.'));
  return number == null || !number.isFinite ? null : number;
}
