import 'package:pr_tools/src/application/ai/description_limits.dart';
import 'package:pr_tools/src/application/ai/description_models.dart';
import 'package:test/test.dart';

void main() {
  test('accepts a PR body below Azure DevOps limit', () {
    final description = PrDescription(title: 'Título', body: 'a' * 3999);

    final result = validateAzurePrDescription(description);

    expect(result.getOrNull(), description);
  });

  test('rejects a PR body at Azure DevOps limit', () {
    final description = PrDescription(title: 'Título', body: 'a' * 4000);

    final result = validateAzurePrDescription(description);

    expect(result.getOrNull(), isNull);
    expect(result.exceptionOrNull(), isA<DescriptionLengthFailure>());
    expect(result.exceptionOrNull()?.message, contains('4000 caracteres'));
  });
}
