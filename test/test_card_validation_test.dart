import 'package:pr_tools/src/features/test_card/test_card_validation.dart';
import 'package:test/test.dart';

void main() {
  group('Test Card validation', () {
    test('allows an empty priority prompt so its default is preserved', () {
      expect(validatePositiveDecimal(''), isNull);
    });

    test('rejects an empty required Azure field before publishing', () {
      expect(validateRequiredText(''), 'Informe um valor.');
      expect(validateRequiredText('DevOps'), isNull);
    });

    test('preserves a configured required field when the prompt is empty', () {
      expect(
        resolveRequiredText('', 'DevOps', 'Custom.Team').getOrNull(),
        'DevOps',
      );
    });
  });
}
