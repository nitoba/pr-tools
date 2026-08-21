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
- O checklist deve ser acionável para QA.
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

TestCardDraft buildCreateTestCaseInput(
  TestCardSettings settings,
  int parentId,
  String title,
  String body,
) => TestCardDraft(
  title: title,
  descriptionHtml: body,
  areaPath: settings.areaPath.isEmpty ? null : settings.areaPath,
  parentId: parentId,
  iterationPath: settings.iterationPath.isEmpty ? null : settings.iterationPath,
  priority: settings.priority,
  team: settings.team.isEmpty ? null : settings.team,
  program: settings.program.isEmpty ? null : settings.program,
  assignedTo: settings.assignedTo.isEmpty ? null : settings.assignedTo,
);

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
