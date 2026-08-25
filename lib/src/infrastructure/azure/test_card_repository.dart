import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../features/test_card/test_card_models.dart';
import '../../features/test_card/test_card_repository.dart';
import 'execution.dart';
import 'pull_requests.dart';
import 'types.dart';
import 'work_items.dart';

final class AzureTestCardRepositoryLive implements TestCardRepository {
  const AzureTestCardRepositoryLive();

  @override
  AppEffect<WorkItem> createTestCase(TestCardDraft input) =>
      Effect.result((use) async {
        final item = await use.unwrap(
          use<AzureWorkItemClient>().createTestCase(
            _project(use),
            _toAzureDraft(input),
          ),
        );
        return _workItem(item);
      });

  @override
  AppEffect<PullRequest> getPullRequest(int id) => Effect.result((use) async {
    final pullRequest = await use.unwrap(
      use<AzurePullRequestClient>().get(_project(use), _repository(use), id),
    );
    return _pullRequest(pullRequest);
  });

  @override
  AppEffect<List<Change>> getPullRequestChanges(PullRequest pullRequest) =>
      Effect.result((use) async {
        final requests = use<AzurePullRequestClient>();
        final iterations = await use.unwrap(
          requests.iterations(_project(use), _repository(use), pullRequest.id),
        );
        if (iterations.isEmpty) return const [];
        final changes = await use.unwrap(
          requests.changes(
            _project(use),
            _repository(use),
            pullRequest.id,
            iterations.last.id,
          ),
        );
        return changes
            .map((change) => Change(type: change.changeType, path: change.path))
            .toList(growable: false);
      });

  @override
  AppEffect<List<int>> getPullRequestWorkItemIds(int id) => Effect.result(
    (use) => use.unwrap(
      use<AzurePullRequestClient>().linkedWorkItemIds(
        _project(use),
        _repository(use),
        id,
      ),
    ),
  );

  @override
  AppEffect<WorkItem> getWorkItem(int id) => Effect.result((use) async {
    final item = await use.unwrap(
      use<AzureWorkItemClient>().get(_project(use), id),
    );
    return _workItem(item);
  });

  @override
  AppEffect<List<int>> queryTestCaseIds() => Effect.result((use) async {
    final project = _project(use);
    final wiql =
        "SELECT [System.Id],[System.Title] FROM WorkItems WHERE "
        "[System.WorkItemType]='Test Case' AND "
        "[System.TeamProject]='${project.replaceAll("'", "''")}' "
        'ORDER BY [System.ChangedDate] DESC';
    return use.unwrap(use<AzureWorkItemClient>().query(project, wiql));
  });

  @override
  AppEffect<TestCardUpdate> updateWorkItemToTestQa(
    int id, {
    num? effort,
    num? realEffort,
  }) => Effect.result((use) async {
    await use.unwrap(
      use<AzureWorkItemClient>().updateToTestQa(
        _project(use),
        id,
        effort: effort,
        realEffort: realEffort,
      ),
    );
    return const TestCardUpdate();
  });
}

String _project(EffectContext use) =>
    use<AzureExecutionContext>().change.remote?.project ?? '';

String _repository(EffectContext use) =>
    use<AzureExecutionContext>().change.remote?.repository ?? '';

WorkItem _workItem(AzureWorkItem item) =>
    WorkItem(id: item.id, fields: item.fields);

PullRequest _pullRequest(AzurePullRequest item) => PullRequest(
  id: item.pullRequestId,
  title: item.title,
  description: item.description,
  sourceBranch: item.sourceRefName,
  targetBranch: item.targetRefName,
);

CreateTestCaseInput _toAzureDraft(TestCardDraft input) => CreateTestCaseInput(
  title: input.title,
  descriptionHtml: input.descriptionHtml,
  stepsXml: input.stepsXml,
  areaPath: input.areaPath,
  parentId: input.parentId,
  iterationPath: input.iterationPath,
  priority: input.priority,
  team: input.team,
  program: input.program,
  assignedTo: input.assignedTo,
);
