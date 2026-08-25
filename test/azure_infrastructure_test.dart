import 'package:better_effect/better_effect.dart';
import 'package:dio/dio.dart';
import 'package:pr_tools/src/app/app_effect.dart';
import 'package:pr_tools/src/app/app_failure.dart';
import 'package:pr_tools/src/application/change_context/merged_pull_request_lookup.dart';
import 'package:pr_tools/src/application/config/config_models.dart';
import 'package:pr_tools/src/domain/change_context.dart';
import 'package:pr_tools/src/features/describe/pull_request_publisher.dart';
import 'package:pr_tools/src/features/test_card/test_card_models.dart';
import 'package:pr_tools/src/features/test_card/test_card_repository.dart';
import 'package:pr_tools/src/infrastructure/azure/index.dart';
import 'package:test/test.dart';

void main() {
  test(
    'publishes through contextual Azure services and request bindings',
    () async {
      final pullRequests = _FakePullRequests(
        repository: const AzureRepository(id: 'repository-id'),
      );
      final workItems = _FakeWorkItems();
      final config = _config();
      final context = _context();
      final runtime = await _runtime();

      try {
        final result = await runtime.runWith(
          _scope(config, context, pullRequests, workItems),
          Effect.result((use) async {
            expect(use<AzureClientOptions>().pat, 'pat');
            expect(use<AzureClientOptions>().organization, 'acme');
            expect(use<Dio>(), isA<Dio>());

            final publisher = use<PullRequestPublisher>();
            return use.unwrap(
              publisher.publish(
                ['dev', 'sprint/98'],
                const PullRequestDraft(
                  title: 'A title',
                  description: 'A body',
                  workItemIds: ['42'],
                ),
                reviewerForTarget: (target) =>
                    target == 'dev' ? ' qa@example.com ' : '',
              ),
            );
          }),
        );
        final published = result.fold(
          (value) => value,
          (failure) => fail((failure as AppFailure).message),
        );

        expect(published, hasLength(2));
        expect(published[0].target, 'dev');
        expect(published[1].target, 'sprint/98');
        expect(pullRequests.repositoryRequests, [('My Project', 'repo')]);
        expect(pullRequests.created, hasLength(2));
        expect(pullRequests.created[0].project, 'My Project');
        expect(pullRequests.created[0].repository, 'repository-id');
        expect(
          pullRequests.created[0].request.sourceRefName,
          'refs/heads/feature/1',
        );
        expect(pullRequests.created[0].request.targetRefName, 'refs/heads/dev');
        expect(
          pullRequests.created[0].request.reviewers!.single.uniqueName,
          'qa@example.com',
        );
        expect(pullRequests.created[0].request.workItemRefs?.single.id, '42');
        expect(pullRequests.created[1].request.reviewers, isNull);
      } finally {
        await runtime.close();
      }
    },
  );

  test('Test Card repository selects the latest iteration and escapes WIQL', () async {
    final pullRequests = _FakePullRequests(
      repository: const AzureRepository(id: 'repository-id'),
      iterations: const [
        AzurePullRequestIteration(id: 2),
        AzurePullRequestIteration(id: 9),
      ],
    );
    final workItems = _FakeWorkItems(queryResult: [11, 12]);
    final config = _config();
    final context = _context();
    final runtime = await _runtime();

    try {
      final result = await runtime.runWith(
        _scope(config, context, pullRequests, workItems),
        Effect.result((use) async {
          final repository = use<TestCardRepository>();
          final changes = await use.unwrap(
            repository.getPullRequestChanges(
              const PullRequest(
                id: 7,
                title: 'A title',
                description: '',
                sourceBranch: 'refs/heads/feature/1',
                targetBranch: 'refs/heads/dev',
              ),
            ),
          );
          final ids = await use.unwrap(repository.queryTestCaseIds());
          return (changes, ids);
        }),
      );
      final values = result.fold(
        (value) => value,
        (failure) => fail((failure as AppFailure).message),
      );

      expect(values.$1.single.path, '/lib/login.dart');
      expect(pullRequests.changeIterations, [9]);
      expect(values.$2, [11, 12]);
      expect(
        workItems.queries.single,
        "SELECT [System.Id],[System.Title] FROM WorkItems WHERE "
        "[System.WorkItemType]='Test Case' AND "
        "[System.TeamProject]='My Project' ORDER BY [System.ChangedDate] DESC",
      );
    } finally {
      await runtime.close();
    }
  });

  test('selects the most recent merged PR across targets', () async {
    final pullRequests = _FakePullRequests(
      repository: const AzureRepository(id: 'repository-id'),
      completedByTarget: {
        'refs/heads/dev': [
          AzurePullRequest(
            pullRequestId: 7,
            title: 'Old',
            description: '',
            sourceRefName: 'refs/heads/feature/1',
            targetRefName: 'refs/heads/dev',
            closedDate: DateTime.utc(2026, 1, 1),
            lastMergeSourceCommit: const AzureCommitRef(commitId: 'oldsha'),
          ),
        ],
        'refs/heads/sprint/98': [
          AzurePullRequest(
            pullRequestId: 8,
            title: 'New',
            description: '',
            sourceRefName: 'refs/heads/feature/1',
            targetRefName: 'refs/heads/sprint/98',
            closedDate: DateTime.utc(2026, 1, 2),
            lastMergeSourceCommit: const AzureCommitRef(commitId: 'newsha'),
          ),
        ],
      },
    );
    final runtime = await _runtime();

    try {
      final result = await runtime.runWith(
        _scope(_config(), _context(), pullRequests, _FakeWorkItems()),
        Effect.result((use) async {
          return use.unwrap(
            use<MergedPullRequestLookup>().findLatest(
              remote: _context().remote,
              sourceBranch: 'feature/1',
              targetBranches: const ['dev', 'sprint/98'],
            ),
          );
        }),
      );
      final lookup = result.fold(
        (value) => value,
        (failure) => fail((failure as AppFailure).message),
      );

      expect(lookup.value?.id, 8);
      expect(lookup.value?.targetBranch, 'sprint/98');
      expect(lookup.value?.sourceCommit, 'newsha');
    } finally {
      await runtime.close();
    }
  });

  test(
    'publisher maps an empty repository ID to a typed Azure failure',
    () async {
      final pullRequests = _FakePullRequests(
        repository: const AzureRepository(id: ''),
      );
      final workItems = _FakeWorkItems();
      final runtime = await _runtime();

      try {
        final result = await runtime.runWith(
          _scope(_config(), _context(), pullRequests, workItems),
          Effect.result((use) async {
            return use.unwrap(
              use<PullRequestPublisher>().publish(const [
                'dev',
              ], const PullRequestDraft(title: 'title', description: 'body')),
            );
          }),
        );

        result.fold((_) => fail('an empty repository ID should fail'), (
          failure,
        ) {
          expect(failure, isA<AzurePayloadError>());
          expect(
            (failure as AppFailure).message,
            'Azure DevOps não retornou o ID do repositório.',
          );
        });
      } finally {
        await runtime.close();
      }
    },
  );
}

Future<Runtime> _runtime() => Module([]).start();

Module _scope(
  Config config,
  ChangeContext context,
  AzurePullRequestClient pullRequests,
  AzureWorkItemClient workItems,
) => azureRequestModule(config, context).overrideWith([
  .instance<AzurePullRequestClient>(pullRequests),
  .instance<AzureWorkItemClient>(workItems),
]);

Config _config() => const Config(
  providers: ['codex'],
  baseUrl: 'https://api.test',
  compatibleModel: 'compatible',
  compatibleReasoning: 'medium',
  codexModel: 'codex',
  codexReasoning: 'medium',
  opencodeModel: 'opencode',
  opencodeReasoning: 'medium',
  azurePat: 'pat',
  reviewerDev: 'dev@example.com',
  reviewerSprint: 'sprint@example.com',
  testAreaPath: '',
  testAssignedTo: '',
  testTeam: '',
  testProgram: '',
  apiKey: 'key',
  template: '',
);

ChangeContext _context() => const ChangeContext(
  branch: 'feature/1',
  sourceRef: 'feature/1',
  baseBranch: 'dev',
  sprintBranch: 'sprint/98',
  diff: '',
  diffOriginalLines: 0,
  log: '',
  workItemId: '42',
  remote: RepositoryRemote(
    organization: 'acme',
    project: 'My Project',
    repository: 'repo',
  ),
);

final class _FakePullRequests implements AzurePullRequestClient {
  _FakePullRequests({
    required this.repository,
    List<AzurePullRequestIteration> iterations = const [],
    this.completedByTarget = const {},
  }) : _iterationValues = iterations;

  final AzureRepository repository;
  final List<AzurePullRequestIteration> _iterationValues;
  final Map<String, List<AzurePullRequest>> completedByTarget;
  final repositoryRequests = <(String, String)>[];
  final created =
      <({String project, String repository, CreatePullRequestInput request})>[];
  final changeIterations = <int>[];

  @override
  AppEffect<List<AzurePullRequestChange>> changes(
    String project,
    String repository,
    int pullRequestId,
    int iterationId,
  ) {
    changeIterations.add(iterationId);
    return Effect.result(
      (_) => const [
        AzurePullRequestChange(changeType: 'edit', path: '/lib/login.dart'),
      ],
    );
  }

  @override
  AppEffect<AzurePullRequest> create(
    String project,
    String repository,
    CreatePullRequestInput request,
  ) {
    created.add((project: project, repository: repository, request: request));
    return Effect.result(
      (_) => AzurePullRequest(
        pullRequestId: created.length,
        title: request.title,
        description: request.description,
        sourceRefName: request.sourceRefName,
        targetRefName: request.targetRefName,
      ),
    );
  }

  @override
  AppEffect<AzurePullRequest> get(
    String project,
    String repository,
    int pullRequestId,
  ) => Effect.result(
    (_) => const AzurePullRequest(
      pullRequestId: 7,
      title: 'A title',
      description: '',
      sourceRefName: 'refs/heads/feature/1',
      targetRefName: 'refs/heads/dev',
    ),
  );

  @override
  AppEffect<List<AzurePullRequest>> completed(
    String project,
    String repository,
    String sourceRefName,
    String targetRefName,
  ) => Effect.succeed(completedByTarget[targetRefName] ?? const []);

  @override
  AppEffect<AzureRepository> getRepository(String project, String repository) {
    repositoryRequests.add((project, repository));
    return Effect.result((_) => this.repository);
  }

  @override
  AppEffect<List<AzurePullRequestIteration>> iterations(
    String project,
    String repository,
    int pullRequestId,
  ) => Effect.result((_) => _iterationValues);

  @override
  AppEffect<List<int>> linkedWorkItemIds(
    String project,
    String repository,
    int pullRequestId,
  ) => Effect.result((_) => const [42]);
}

final class _FakeWorkItems implements AzureWorkItemClient {
  _FakeWorkItems({this.queryResult = const []});

  final List<int> queryResult;
  final queries = <String>[];

  @override
  AppEffect<AzureWorkItem> get(String project, int id) =>
      Effect.result((_) => const AzureWorkItem(id: 42, fields: {}));

  @override
  AppEffect<List<int>> query(String project, String wiql) {
    queries.add(wiql);
    return Effect.result((_) => queryResult);
  }

  @override
  AppEffect<AzureUnit> updateState(String project, int id, String state) =>
      Effect.result((_) => const AzureUnit());

  @override
  AppEffect<AzureUnit> updateToTestQa(
    String project,
    int id, {
    num? effort,
    num? realEffort,
  }) => Effect.result((_) => const AzureUnit());

  @override
  AppEffect<AzureWorkItem> createTestCase(
    String project,
    CreateTestCaseInput input,
  ) => Effect.result((_) => const AzureWorkItem(id: 99, fields: {}));
}
