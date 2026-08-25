import 'dart:convert';
import 'dart:typed_data';

import 'package:better_effect/better_effect.dart';
import 'package:dio/dio.dart';
import 'package:pr_tools/src/app/app_effect.dart';
import 'package:pr_tools/src/infrastructure/azure/index.dart';
import 'package:test/test.dart';

void main() {
  test('encodes Azure URLs and preserves Dio auth/body semantics', () async {
    final adapter = _ResponseAdapter(
      status: 200,
      body: '{"id":99,"fields":{"System.Title":"Teste"}}',
    );
    final dio = Dio()..httpClientAdapter = adapter;
    final module = Module([
      .instance(
        const AzureClientOptions(
          pat: 'test-pat',
          organization: 'acme',
          baseUrl: 'https://azure.test/',
        ),
      ),
      .instance<Dio>(dio),
      .provide<AzureHttp>(AzureHttpLive.new),
    ]);

    final response = await _run(
      module,
      Effect.result((use) async {
        return use.unwrap(
          use<AzureHttp>().send(
            const AzureRequest(
              '/My%20Project/_apis/wit/workitems/42',
              method: 'POST',
              body: {'title': 'Teste'},
            ),
          ),
        );
      }),
    );

    expect(response.data, {
      'id': 99,
      'fields': {'System.Title': 'Teste'},
    });
    expect(
      adapter.options?.uri.toString(),
      'https://azure.test/My%20Project/_apis/wit/workitems/42',
    );
    expect(adapter.options?.method, 'POST');
    expect(adapter.options?.headers['Accept'], 'application/json');
    expect(
      adapter.options?.headers['Authorization'],
      'Basic ${base64Encode(utf8.encode(':test-pat'))}',
    );
    expect(adapter.options?.headers['Content-Type'], 'application/json');
    expect(adapter.requestBody, '{"title":"Teste"}');
  });

  test('maps HTTP status and trimmed response body to AzureApiError', () async {
    final module = Module([
      .instance(
        const AzureClientOptions(pat: 'test-pat', organization: 'acme'),
      ),
      .instance<Dio>(
        Dio()
          ..httpClientAdapter = _ResponseAdapter(
            status: 403,
            body: '  {"message":"denied"}  ',
          ),
      ),
      .provide<AzureHttp>(AzureHttpLive.new),
    ]);

    final result = await module.run(
      Effect.result((use) async {
        return use.unwrap(
          use<AzureHttp>().send(const AzureRequest('/projects')),
        );
      }),
    );

    result.fold((_) => fail('a chamada deveria falhar'), (failure) {
      expect(failure, isA<AzureApiError>());
      final error = failure as AzureApiError;
      expect(error.status, 403);
      expect(error.responseBody, '{"message":"denied"}');
      expect(
        error.message,
        'Azure DevOps API respondeu 403: {"message":"denied"}',
      );
    });
  });

  test('creates PRs and decodes linked Work Item IDs through contextual services', () async {
    final http = _RecordingHttp((request) {
      if (request.path.contains('/repositories/repo?')) {
        return {'id': 'repo-id'};
      }
      if (request.path.contains('/workitems')) {
        return {
          'value': [
            {'id': '123'},
            {'id': 456},
          ],
        };
      }
      return {
        'pullRequestId': 7,
        'title': 'A title',
        'description': 'A body',
        'sourceRefName': 'refs/heads/feature/1',
        'targetRefName': 'refs/heads/dev',
      };
    });
    final module = _apiModule(http);

    final created = await _run(
      module,
      Effect.result((use) async {
        return use.unwrap(
          use<AzurePullRequestClient>().create(
            'My Project',
            'repo',
            const CreatePullRequestInput(
              title: 'A title',
              description: 'A body',
              sourceRefName: 'refs/heads/feature/1',
              targetRefName: 'refs/heads/dev',
              reviewers: [AzureIdentity(id: 'identity-id')],
              workItemRefs: [AzureResourceRef(id: '123')],
            ),
          ),
        );
      }),
    );
    final ids = await _run(
      module,
      Effect.result((use) async {
        return use.unwrap(
          use<AzurePullRequestClient>().linkedWorkItemIds(
            'My Project',
            'repo',
            7,
          ),
        );
      }),
    );

    expect(created.pullRequestId, 7);
    expect(ids, [123, 456]);
    expect(http.requests, hasLength(2));
    expect(
      http.requests.first.path,
      '/My%20Project/_apis/git/repositories/repo/pullrequests?api-version=7.1',
    );
    expect(http.requests.first.body, {
      'title': 'A title',
      'description': 'A body',
      'sourceRefName': 'refs/heads/feature/1',
      'targetRefName': 'refs/heads/dev',
      'reviewers': [
        {'id': 'identity-id'},
      ],
      'workItemRefs': [
        {'id': '123'},
      ],
    });
    expect(
      http.requests[1].path,
      '/My%20Project/_apis/git/repositories/repo/pullRequests/7/workitems?api-version=7.1',
    );
  });

  test('lists completed PRs with their merge source commits', () async {
    final http = _RecordingHttp(
      (_) => {
        'value': [
          {
            'pullRequestId': 17,
            'title': 'A title',
            'description': 'A body',
            'sourceRefName': 'refs/heads/feature/1',
            'targetRefName': 'refs/heads/dev',
            'status': 'completed',
            'closedDate': '2026-01-02T03:04:05Z',
            'lastMergeSourceCommit': {'commitId': 'abc123'},
          },
        ],
      },
    );
    final pullRequests = await _run(
      _apiModule(http),
      Effect.result((use) async {
        return use.unwrap(
          use<AzurePullRequestClient>().completed(
            'My Project',
            'repo',
            'refs/heads/feature/1',
            'refs/heads/dev',
          ),
        );
      }),
    );

    expect(pullRequests.single.pullRequestId, 17);
    expect(pullRequests.single.lastMergeSourceCommit?.commitId, 'abc123');
    expect(pullRequests.single.closedDate, DateTime.utc(2026, 1, 2, 3, 4, 5));
    expect(
      http.requests.single.path,
      allOf(
        contains('searchCriteria.status=completed'),
        contains('searchCriteria.sourceRefName=refs%2Fheads%2Ffeature%2F1'),
        contains('searchCriteria.targetRefName=refs%2Fheads%2Fdev'),
        contains('%24top=100'),
        endsWith('api-version=7.1'),
      ),
    );
  });

  test('resolves reviewer emails to Azure identity IDs', () async {
    final http = _RecordingHttp(
      (_) => {
        'value': [
          {'id': 'identity-id'},
        ],
      },
    );
    final identity = await _run(
      _apiModule(http),
      Effect.result((use) async {
        return use.unwrap(
          use<AzureIdentityClient>().resolve('qa+reviewer@example.com'),
        );
      }),
    );

    expect(identity.id, 'identity-id');
    expect(http.requests.single.baseUrl, 'https://vssps.dev.azure.com/acme');
    expect(
      http.requests.single.path,
      allOf(
        contains('searchFilter=General'),
        contains('filterValue=qa%2Breviewer%40example.com'),
        contains('queryMembership=None'),
        endsWith('api-version=7.1'),
      ),
    );
  });

  test('maps malformed Azure DTOs to AzurePayloadError', () async {
    final module = _apiModule(
      _RecordingHttp(
        (_) => {
          'pullRequestId': 7,
          'title': 42,
          'description': 'A body',
          'sourceRefName': 'refs/heads/feature/1',
          'targetRefName': 'refs/heads/dev',
        },
      ),
    );
    final result = await module.run(
      Effect.result(
        (use) => use.unwrap(
          use<AzurePullRequestClient>().get('My Project', 'repo', 7),
        ),
      ),
    );

    result.fold((_) => fail('a chamada deveria falhar'), (failure) {
      expect(failure, isA<AzurePayloadError>());
    });
  });

  test('builds Test Case JSON Patch and Test QA updates exactly', () async {
    final http = _RecordingHttp((request) {
      if (request.method == 'POST') {
        return {
          'id': 99,
          'fields': {'System.Title': 'Teste login'},
        };
      }
      return null;
    });
    final module = _apiModule(http);

    final created = await _run(
      module,
      Effect.result((use) async {
        return use.unwrap(
          use<AzureWorkItemClient>().createTestCase(
            'My Project',
            const CreateTestCaseInput(
              title: 'Teste login',
              descriptionHtml: '<p>Validar login.</p>',
              areaPath: r'Project\QA',
              parentId: 42,
              iterationPath: r'Project\Sprint 98',
              priority: 2,
              team: 'DevOps',
              program: 'Agrotrace',
              assignedTo: 'qa@example.com',
            ),
          ),
        );
      }),
    );
    await _run(
      module,
      Effect.result((use) async {
        return use.unwrap(
          use<AzureWorkItemClient>().updateToTestQa(
            'My Project',
            42,
            effort: 0.5,
            realEffort: 1,
          ),
        );
      }),
    );

    expect(created.id, 99);
    expect(http.requests[0].contentType, 'application/json-patch+json');
    expect(
      http.requests[0].path,
      '/My%20Project/_apis/wit/workitems/%24Test%20Case?api-version=7.1',
    );
    expect(http.requests[0].body, [
      {'op': 'add', 'path': '/fields/System.Title', 'value': 'Teste login'},
      {
        'op': 'add',
        'path': '/fields/System.Description',
        'value': '<p>Validar login.</p>',
      },
      {'op': 'add', 'path': '/fields/System.AreaPath', 'value': r'Project\QA'},
      {
        'op': 'add',
        'path': '/fields/System.IterationPath',
        'value': r'Project\Sprint 98',
      },
      {
        'op': 'add',
        'path': '/fields/Microsoft.VSTS.Common.Priority',
        'value': 2,
      },
      {'op': 'add', 'path': '/fields/Custom.Team', 'value': 'DevOps'},
      {
        'op': 'add',
        'path': '/fields/Custom.ProgramasAgrotrace',
        'value': 'Agrotrace',
      },
      {
        'op': 'add',
        'path': '/fields/System.AssignedTo',
        'value': 'qa@example.com',
      },
      {
        'op': 'add',
        'path': '/relations/-',
        'value': {
          'rel': 'System.LinkTypes.Hierarchy-Reverse',
          'url': 'https://dev.azure.com/acme/_apis/wit/workitems/42',
        },
      },
    ]);
    expect(http.requests[1].body, [
      {'op': 'add', 'path': '/fields/System.State', 'value': 'Test QA'},
      {
        'op': 'add',
        'path': '/fields/Microsoft.VSTS.Scheduling.Effort',
        'value': 0.5,
      },
      {'op': 'add', 'path': '/fields/Custom.RealEffort', 'value': 1},
    ]);
  });
}

Module _apiModule(_RecordingHttp http) => Module([
  .instance(const AzureClientOptions(pat: 'test-pat', organization: 'acme')),
  .instance<AzureHttp>(http),
  .provide<AzureDevOpsClient>(AzureDevOpsClientLive.new),
  .provide<AzurePullRequestClient>(AzurePullRequestClientLive.new),
  .provide<AzureIdentityClient>(AzureIdentityClientLive.new),
  .provide<AzureWorkItemClient>(AzureWorkItemClientLive.new),
]);

Future<T> _run<T extends Object>(Module module, AppEffect<T> effect) async {
  final result = await module.run(effect);
  return result.fold(
    (value) => value,
    (failure) => throw StateError(failure.message),
  );
}

final class _RecordingHttp implements AzureHttp {
  _RecordingHttp(this._handler);

  final Object? Function(AzureRequest request) _handler;
  final requests = <AzureRequest>[];

  @override
  AppEffect<AzureHttpResponse> send(AzureRequest request) => Effect.result((_) {
    requests.add(request);
    final data = _handler(request);
    return AzureHttpResponse(status: 200, body: '', data: data);
  });
}

final class _ResponseAdapter implements HttpClientAdapter {
  _ResponseAdapter({required this.status, required this.body});

  final int status;
  final String body;
  RequestOptions? options;
  String? requestBody;

  @override
  Future<ResponseBody> fetch(
    RequestOptions options,
    Stream<Uint8List>? requestStream,
    Future<void>? cancelFuture,
  ) async {
    this.options = options;
    if (requestStream != null) {
      final chunks = await requestStream.toList();
      final bytes = <int>[];
      for (final chunk in chunks) {
        bytes.addAll(chunk);
      }
      requestBody = utf8.decode(bytes);
    }
    return ResponseBody.fromString(body, status);
  }

  @override
  void close({bool force = false}) {}
}
