import '../../application/ai/description_models.dart';
import '../../application/config/config_models.dart';
import '../../domain/change_context.dart';

const testCardSystemPrompt = '''Você é um analista de QA técnico.

Gere um card de teste em português brasileiro para Azure DevOps com base no Work Item pai, no PR, nas alterações e nos exemplos fornecidos.

Retorne um objeto JSON com exatamente estes campos:
- "title": título curto, objetivo e testável.
- "body": Markdown com estas seções, nesta ordem:
  - ## Objetivo
  - ## Cenário base
  - ## Checklist de testes
  - ## Resultado esperado

Regras:
- Não invente comportamento que não esteja sustentado pelo contexto.
- Foque em cobertura funcional, validações e regressão.
- O checklist deve ser uma lista de passos executáveis, com um item por passo.
- O resultado esperado deve ser explícito e verificável; ele será associado aos passos no Test Case.
- Não cite nomes de arquivos, classes, funções, APIs internas ou detalhes de implementação.
- Descreva apenas cenários observáveis e validáveis pelo usuário final ou pelo analista de QA.''';

/// Kept as a public alias for callers ported from the TypeScript feature.
const testCardSystemPromptValue = testCardSystemPrompt;

final class WorkItem {
  const WorkItem({required this.id, required this.fields});

  final int id;
  final Map<String, Object?> fields;
}

final class PullRequest {
  const PullRequest({
    required this.id,
    required this.title,
    required this.description,
    required this.sourceBranch,
    required this.targetBranch,
  });

  final int id;
  final String title;
  final String description;
  final String sourceBranch;
  final String targetBranch;
}

final class Change {
  const Change({required this.type, required this.path});

  final String type;
  final String path;
}

final class TestCardDraft {
  const TestCardDraft({
    required this.title,
    this.descriptionHtml,
    this.stepsXml,
    this.areaPath,
    this.parentId,
    this.iterationPath,
    this.priority,
    this.team,
    this.program,
    this.assignedTo,
  });

  final String title;
  final String? descriptionHtml;
  final String? stepsXml;
  final String? areaPath;
  final int? parentId;
  final String? iterationPath;
  final num? priority;
  final String? team;
  final String? program;
  final String? assignedTo;
}

final class TestCardUpdate {
  const TestCardUpdate();
}

final class TestCardSettings {
  const TestCardSettings({
    required this.areaPath,
    required this.assignedTo,
    required this.iterationPath,
    required this.priority,
    required this.team,
    required this.program,
  });

  final String areaPath;
  final String assignedTo;
  final String iterationPath;
  final num priority;
  final String team;
  final String program;
}

final class TestCardContext {
  const TestCardContext({
    required this.change,
    required this.workItem,
    required this.changes,
    required this.examples,
    this.pullRequest,
  });

  final ChangeContext change;
  final WorkItem workItem;
  final PullRequest? pullRequest;
  final List<Change> changes;
  final List<String> examples;
}

final class TestCardPreparation {
  const TestCardPreparation({
    required this.config,
    required this.context,
    required this.prompt,
  });

  final Config config;
  final TestCardContext context;
  final String prompt;
}

String workItemText(WorkItem item, String field) {
  final value = item.fields[field];
  return value is String ? value : '';
}

num? workItemNumber(WorkItem item, String field) {
  final value = item.fields[field];
  if (value is num && value.isFinite) return value;
  if (value is String && value.trim().isNotEmpty) {
    final number = num.tryParse(value.trim().replaceAll(',', '.'));
    if (number != null && number.isFinite) return number;
  }
  return null;
}

int? selectParentWorkItem(List<WorkItem> workItems) {
  final ordered = [...workItems]
    ..sort((left, right) => left.id.compareTo(right.id));
  for (final item in ordered) {
    if (workItemText(item, 'System.WorkItemType') != 'Test Case') {
      return item.id;
    }
  }
  return ordered.isEmpty ? null : ordered.first.id;
}

String buildTestCardPrompt(TestCardContext context) {
  final workItem = context.workItem;
  final change = context.change;
  final lines = <String>[
    '## Contexto do Work Item',
    '',
    'ID: ${workItem.id}',
    'Título: ${workItemText(workItem, 'System.Title')}',
    'Tipo: ${workItemText(workItem, 'System.WorkItemType')}',
  ];
  final areaPath = workItemText(workItem, 'System.AreaPath');
  final description = workItemText(workItem, 'System.Description');
  if (areaPath.isNotEmpty) lines.add('Área: $areaPath');
  if (description.isNotEmpty) lines.add('Descrição: $description');

  final pullRequest = context.pullRequest;
  if (pullRequest != null) {
    lines.addAll([
      '',
      '## Contexto do PR',
      '',
      'PR ID: ${pullRequest.id}',
      'Título: ${pullRequest.title}',
      'Branch origem: ${pullRequest.sourceBranch}',
      'Branch destino: ${pullRequest.targetBranch}',
    ]);
    if (pullRequest.description.isNotEmpty) {
      lines.add('Descrição: ${pullRequest.description}');
    }
  }

  lines.addAll([
    '',
    '## Contexto Git',
    '',
    'Branch atual: ${change.branch}',
    'Base: ${change.baseBranch}',
  ]);
  if (context.changes.isNotEmpty) {
    lines.addAll(['', '## Arquivos alterados']);
    for (final change in context.changes) {
      lines.add('- [${change.type}] ${change.path}');
    }
  }
  if (change.diff.isNotEmpty) {
    lines.addAll(['', '## Diff resumido', '', '```diff', change.diff, '```']);
  }
  if (change.log.isNotEmpty) {
    lines.addAll(['', '## Commits', '', '```', change.log, '```']);
  }
  if (context.examples.isNotEmpty) {
    lines.addAll(['', '## Exemplos de Test Case', '']);
    lines.addAll(context.examples);
  }
  lines.addAll([
    '',
    '## Instruções finais',
    '',
    'Gere o card conforme o formato definido no system prompt.',
  ]);
  return '${lines.join('\n')}\n';
}

String buildTestCaseStepsXml(String body) {
  final checklist = _markdownSection(body, 'Checklist de testes');
  final actions = _markdownListItems(checklist);
  if (actions.isEmpty) return '';

  final expectedSection = _markdownSection(body, 'Resultado esperado');
  final expectedResults = _expectedResults(expectedSection, actions.length);
  final xml = StringBuffer('<steps id="0" last="${actions.length + 1}">');
  for (var index = 0; index < actions.length; index++) {
    final expectedResult = expectedResults[index];
    final type = expectedResult.isEmpty ? 'ActionStep' : 'ValidateStep';
    xml
      ..write('\n  <step id="${index + 2}" type="$type">')
      ..write(
        '\n    <parameterizedString isformatted="true">${_formattedStepValue(actions[index])}</parameterizedString>',
      )
      ..write(
        '\n    <parameterizedString isformatted="true">${_formattedStepValue(expectedResult)}</parameterizedString>',
      )
      ..write('\n    <description/>')
      ..write('\n  </step>');
  }
  xml.write('\n</steps>');
  return xml.toString();
}

TestCardDraft buildCreateTestCaseInput(
  TestCardSettings settings,
  int parentId,
  String title,
  String body,
) {
  final stepsXml = buildTestCaseStepsXml(body);
  return TestCardDraft(
    title: title,
    descriptionHtml: body,
    stepsXml: stepsXml.isEmpty ? null : stepsXml,
    areaPath: settings.areaPath.isEmpty ? null : settings.areaPath,
    parentId: parentId,
    iterationPath: settings.iterationPath.isEmpty
        ? null
        : settings.iterationPath,
    priority: settings.priority,
    team: settings.team.isEmpty ? null : settings.team,
    program: settings.program.isEmpty ? null : settings.program,
    assignedTo: settings.assignedTo.isEmpty ? null : settings.assignedTo,
  );
}

String _markdownSection(String body, String heading) {
  final headingMatch = RegExp(
    r'^\s*##\s+' + RegExp.escape(heading) + r'\s*$',
    caseSensitive: false,
    multiLine: true,
  ).firstMatch(body);
  if (headingMatch == null) return '';

  final contentStart = headingMatch.end;
  final remaining = body.substring(contentStart);
  final nextHeading = RegExp(
    r'^\s*##\s+.+$',
    multiLine: true,
  ).firstMatch(remaining);
  final contentEnd = nextHeading == null
      ? body.length
      : contentStart + nextHeading.start;
  return body.substring(contentStart, contentEnd).trim();
}

List<String> _markdownListItems(String section) {
  if (section.trim().isEmpty) return const [];
  final items = <String>[];
  String? current;
  final itemPattern = RegExp(
    r'^\s*(?:[-*+•]\s+|\d+[.)]\s+)(?:\[[ xX]\]\s*)?(.+?)\s*$',
  );
  for (final line in section.split('\n')) {
    final itemMatch = itemPattern.firstMatch(line);
    if (itemMatch != null) {
      if (current != null) items.add(current);
      current = itemMatch.group(1)!.trim();
    } else if (line.trim().isNotEmpty && current != null) {
      current = '$current ${line.trim()}';
    }
  }
  if (current != null) items.add(current);
  if (items.isNotEmpty) return items;
  return section
      .split('\n')
      .map((line) => line.trim())
      .where((line) => line.isNotEmpty)
      .toList(growable: false);
}

List<String> _expectedResults(String section, int count) {
  final items = _markdownListItems(section);
  if (items.length == count) return items;

  final results = List<String>.filled(count, '');
  final fallback = items.isEmpty ? section.trim() : items.join('\n');
  if (fallback.isNotEmpty) results[count - 1] = fallback;
  return results;
}

String _formattedStepValue(String value) {
  final content = value.trim().isEmpty
      ? '&nbsp;'
      : _escapeHtml(value).replaceAll('\n', '<BR/>');
  return _escapeXml('<DIV><P>$content</P></DIV>');
}

String _escapeHtml(String value) => value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;');

String _escapeXml(String value) => value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;');

String workItemUrl(ChangeContext context, int id) {
  final remote = context.remote;
  if (remote == null) return '';
  final organization = Uri.encodeComponent(remote.organization);
  final project = Uri.encodeComponent(remote.project);
  return 'https://dev.azure.com/$organization/$project/_workitems/edit/$id';
}

GeneratedDescription generatedDescription({
  required String title,
  required String body,
  required String provider,
  required String model,
}) => GeneratedDescription(
  description: PrDescription(title: title, body: body),
  provider: provider,
  model: model,
);
