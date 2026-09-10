import 'package:better_effect/better_effect.dart';

import '../../application/ai/description_generator.dart';
import '../../application/ai/description_models.dart';
import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import '../../app/cli_options.dart';
import '../../application/config/config_service.dart';
import '../../application/change_context/change_context_reader.dart';
import '../../application/terminal/terminal_ports.dart';
import 'test_card_models.dart';
import 'test_card_repository.dart';
import 'test_card_validation.dart';

abstract interface class TestCardService {
  AppEffect<TestCardPreparation> prepare(CliOptions options, bool interactive);

  AppEffect<GeneratedDescription> generate(
    TestCardPreparation preparation, {
    DescriptionReporter? report,
  });

  AppEffect<WorkItem> create(
    TestCardPreparation preparation,
    TestCardDraft input,
  );

  AppEffect<TestCardUpdate> updateParent(
    TestCardPreparation preparation, {
    num? effort,
    num? realEffort,
  });
}

final class TestCardFailure extends AppFailure {
  const TestCardFailure(String message) : super(message, 1);
}

final class TestCardServiceLive implements TestCardService {
  const TestCardServiceLive();

  @override
  AppEffect<TestCardPreparation> prepare(
    CliOptions options,
    bool interactive,
  ) => .result((use) async {
    final config = await use.unwrap(use<ConfigService>().load(options));
    if (config.azurePat.trim().isEmpty) {
      use.fail(
        const TestCardFailure(
          'O comando test requer AZURE_PAT ou AZURE_DEVOPS_PAT configurado.',
        ),
      );
    }
    if (options.create && !interactive) {
      use.fail(
        const TestCardFailure(
          '--create requer terminal interativo para confirmar o card.',
        ),
      );
    }

    final change = await use.unwrap(
      use<ChangeContextReader>().collect(options.source),
    );
    if (change.remote == null) {
      use.fail(
        const TestCardFailure(
          'O comando test requer um remote Git do Azure DevOps.',
        ),
      );
    }

    final repository = use<TestCardRepository>();
    final pullRequestId = options.pr == null
        ? null
        : int.parse(
            await use.result(parseWorkItemId(options.pr!.value, '--pr')),
          );
    final pullRequest = pullRequestId == null
        ? null
        : await use.unwrap(repository.getPullRequest(pullRequestId));

    String? workItemId = options.workItem?.value;
    if (workItemId != null) {
      workItemId = await use.result(parseWorkItemId(workItemId, '--work-item'));
    }
    if (workItemId == null && change.workItemId.isNotEmpty) {
      workItemId = await use.result(
        parseWorkItemId(change.workItemId, 'branch'),
      );
    }
    if (workItemId == null && pullRequest != null) {
      final linkedIds = await use.unwrap(
        repository.getPullRequestWorkItemIds(pullRequest.id),
      );
      final linkedItems = <WorkItem>[];
      for (final id in linkedIds) {
        final result = await use.unwrap(repository.getWorkItem(id).either());
        result.fold<void>(linkedItems.add, (_) {});
      }
      final parent = selectParentWorkItem(linkedItems);
      workItemId = parent?.toString();
    }
    if (workItemId == null && interactive) {
      final prompts = use<PromptPort>();
      final value = prompts.text(
        message: 'ID do Work Item pai',
        validate: validateWorkItemId,
      );
      if (value == null) use.fail(const TestCardFailure('Operação cancelada.'));
      workItemId = await use.result(parseWorkItemId(value, 'Work Item'));
    }
    if (workItemId == null) {
      use.fail(
        const TestCardFailure(
          'Não foi possível resolver o Work Item pai; use --work-item explicitamente.',
        ),
      );
    }

    final id = int.parse(workItemId);
    final workItem = await use.unwrap(repository.getWorkItem(id));
    final changes = <Change>[];
    if (pullRequest != null) {
      final result = await use.unwrap(
        repository.getPullRequestChanges(pullRequest).either(),
      );
      result.fold<void>(changes.addAll, (_) {});
    }

    final count = await use.result(parseExamplesCount(options.examples));
    final examples = <String>[];
    if (count > 0) {
      final idsResult = await use.unwrap(
        repository.queryTestCaseIds().either(),
      );
      List<int>? ids;
      idsResult.fold<void>((value) => ids = value, (_) {});
      for (final exampleId in (ids ?? const <int>[]).take(count)) {
        final itemResult = await use.unwrap(
          repository.getWorkItem(exampleId).either(),
        );
        itemResult.fold<void>((item) {
          final title = workItemText(item, 'System.Title');
          if (title.isNotEmpty) examples.add('- #$exampleId $title');
        }, (_) {});
      }
    }

    final context = TestCardContext(
      change: change,
      workItem: workItem,
      pullRequest: pullRequest,
      changes: List.unmodifiable(changes),
      examples: List.unmodifiable(examples),
    );
    return TestCardPreparation(
      config: config,
      context: context,
      prompt: buildTestCardPrompt(context),
    );
  });

  @override
  AppEffect<GeneratedDescription> generate(
    TestCardPreparation preparation, {
    DescriptionReporter? report,
  }) => .result((use) async {
    final generator = use<DescriptionGenerator>();
    return use.unwrap(
      generator.generate(
        config: preparation.config,
        system: testCardSystemPromptValue,
        prompt: preparation.prompt,
        branch: 'test-card/${preparation.context.workItem.id}',
        report: report,
      ),
    );
  });

  @override
  AppEffect<WorkItem> create(
    TestCardPreparation preparation,
    TestCardDraft input,
  ) => .result((use) async {
    final repository = use<TestCardRepository>();
    return use.unwrap(repository.createTestCase(input));
  });

  @override
  AppEffect<TestCardUpdate> updateParent(
    TestCardPreparation preparation, {
    num? effort,
    num? realEffort,
  }) => .result((use) async {
    final repository = use<TestCardRepository>();
    return use.unwrap(
      repository.updateWorkItemToTestQa(
        preparation.context.workItem.id,
        effort: effort,
        realEffort: realEffort,
      ),
    );
  });
}
