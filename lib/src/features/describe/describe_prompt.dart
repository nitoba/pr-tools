import '../../domain/change_context.dart';
import 'describe_models.dart';

String buildDescribePrompt(DescribePrompt input) {
  return buildPrompt(input.context, input.targets, input.workItemId);
}

String buildPrompt(
  ChangeContext context,
  List<String> targets,
  String workItemId,
) {
  const fence = '\u0060\u0060\u0060';
  return '''## Contexto Git

**Branch:** ${context.branch}
**Base branches alvo:** ${targets.join(', ')}
${workItemId.isNotEmpty ? '**Work Item:** #$workItemId\n' : ''}### Git Log (commits desde a base)

$fence
${context.log}
$fence

### Git Diff

${fence}diff
${context.diff}
$fence
''';
}
