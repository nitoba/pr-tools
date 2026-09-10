import 'package:pr_tools/pr_tools.dart';
import 'package:test/test.dart';

void main() {
  test('preserves repeated targets and semantic work-item IDs', () {
    final result = parseCli([
      'desc',
      '--target',
      'dev',
      '--target=sprint',
      '--work-item',
      '11763',
    ]);

    expect(result, isA<ParsedOptions>());
    final options = (result as ParsedOptions).options;
    expect(options.targets, ['dev', 'sprint']);
    expect(options.workItem?.value, '11763');
  });

  test('rejects conflicting creation flags', () {
    expect(
      parseCli(['test', '--create', '--no-create']),
      isA<InvalidArguments>(),
    );
  });

  test(
    'validates provider, targets, and work item IDs at the CLI boundary',
    () {
      expect(
        parseCli(['desc', '--provider', 'unknown']),
        isA<InvalidArguments>(),
      );
      expect(parseCli(['desc', '--target', 'main']), isA<InvalidArguments>());
      expect(parseCli(['test', '--work-item', '0']), isA<InvalidArguments>());
    },
  );
}
