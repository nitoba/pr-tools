import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import 'client.dart';
import 'types.dart';

int? _integerId(Object? value) {
  final number = switch (value) {
    num number => number.toDouble(),
    String text when text.trim().isEmpty => 0,
    String text => double.tryParse(text),
    _ => null,
  };
  if (number == null ||
      !number.isFinite ||
      number != number.truncateToDouble()) {
    return null;
  }
  return number.toInt();
}

abstract interface class AzurePullRequestClient {
  AppEffect<List<AzurePullRequestChange>> changes(
    String project,
    String repository,
    int pullRequestId,
    int iterationId,
  );

  AppEffect<AzurePullRequest> create(
    String project,
    String repository,
    CreatePullRequestInput request,
  );

  AppEffect<AzurePullRequest> get(
    String project,
    String repository,
    int pullRequestId,
  );

  AppEffect<AzureRepository> getRepository(String project, String repository);

  AppEffect<List<AzurePullRequestIteration>> iterations(
    String project,
    String repository,
    int pullRequestId,
  );

  AppEffect<List<int>> linkedWorkItemIds(
    String project,
    String repository,
    int pullRequestId,
  );
}

final class AzurePullRequestClientLive implements AzurePullRequestClient {
  const AzurePullRequestClientLive();

  @override
  AppEffect<List<AzurePullRequestChange>> changes(
    String project,
    String repository,
    int pullRequestId,
    int iterationId,
  ) => Effect.result((use) async {
    final client = use<AzureDevOpsClient>();
    final response = await use.unwrap(
      client.request(
        AzureRequest(
          withApiVersion(
            '/${pathSegment(project)}/_apis/git/repositories/${pathSegment(repository)}/pullRequests/$pullRequestId/iterations/$iterationId/changes?${r'$top'}=200',
            '7.0',
          ),
        ),
      ),
    );
    final data = objectMap(response.data);
    final rawItems = data?['changeEntries'];
    if (rawItems is! List) {
      use.fail(
        const AzurePayloadError('Azure não retornou alterações válidas.'),
      );
    }
    return use.unwrap(
      _decode(
        () => rawItems
            .map(objectMap)
            .whereType<Map<String, Object?>>()
            .map(AzurePullRequestChange.fromJson)
            .take(50)
            .toList(growable: false),
      ),
    );
  });

  @override
  AppEffect<AzurePullRequest> create(
    String project,
    String repository,
    CreatePullRequestInput request,
  ) => Effect.result((use) async {
    final client = use<AzureDevOpsClient>();
    final response = await use.unwrap(
      client.request(
        AzureRequest(
          withApiVersion(
            '/${pathSegment(project)}/_apis/git/repositories/${pathSegment(repository)}/pullrequests',
          ),
          method: 'POST',
          body: request.toJson(),
        ),
      ),
    );
    final data = objectMap(response.data);
    if (data == null) {
      use.fail(const AzurePayloadError('Azure não retornou o PR criado.'));
    }
    return use.unwrap(_decode(() => AzurePullRequest.fromJson(data)));
  });

  @override
  AppEffect<AzurePullRequest> get(
    String project,
    String repository,
    int pullRequestId,
  ) => Effect.result((use) async {
    final client = use<AzureDevOpsClient>();
    final response = await use.unwrap(
      client.request(
        AzureRequest(
          withApiVersion(
            '/${pathSegment(project)}/_apis/git/repositories/${pathSegment(repository)}/pullRequests/$pullRequestId',
          ),
        ),
      ),
    );
    final data = objectMap(response.data);
    if (data == null) {
      use.fail(const AzurePayloadError('Azure não retornou um PR válido.'));
    }
    return use.unwrap(_decode(() => AzurePullRequest.fromJson(data)));
  });

  @override
  AppEffect<AzureRepository> getRepository(
    String project,
    String repository,
  ) => Effect.result((use) async {
    final client = use<AzureDevOpsClient>();
    final response = await use.unwrap(
      client.request(
        AzureRequest(
          withApiVersion(
            '/${pathSegment(project)}/_apis/git/repositories/${pathSegment(repository)}',
          ),
        ),
      ),
    );
    final data = objectMap(response.data);
    if (data == null) {
      use.fail(
        const AzurePayloadError('Azure não retornou um repositório válido.'),
      );
    }
    return use.unwrap(_decode(() => AzureRepository.fromJson(data)));
  });

  @override
  AppEffect<List<AzurePullRequestIteration>> iterations(
    String project,
    String repository,
    int pullRequestId,
  ) => Effect.result((use) async {
    final client = use<AzureDevOpsClient>();
    final response = await use.unwrap(
      client.request(
        AzureRequest(
          withApiVersion(
            '/${pathSegment(project)}/_apis/git/repositories/${pathSegment(repository)}/pullRequests/$pullRequestId/iterations',
            '7.0',
          ),
        ),
      ),
    );
    final data = objectMap(response.data);
    final rawItems = data?['value'];
    if (rawItems is! List) {
      use.fail(
        const AzurePayloadError('Azure não retornou iterações válidas.'),
      );
    }
    return use.unwrap(
      _decode(
        () => rawItems
            .map(objectMap)
            .whereType<Map<String, Object?>>()
            .map(AzurePullRequestIteration.fromJson)
            .toList(growable: false),
      ),
    );
  });

  @override
  AppEffect<List<int>> linkedWorkItemIds(
    String project,
    String repository,
    int pullRequestId,
  ) => Effect.result((use) async {
    final client = use<AzureDevOpsClient>();
    final response = await use.unwrap(
      client.request(
        AzureRequest(
          withApiVersion(
            '/${pathSegment(project)}/_apis/git/repositories/${pathSegment(repository)}/pullRequests/$pullRequestId/workitems',
          ),
        ),
      ),
    );
    final data = objectMap(response.data);
    final rawItems = data?['value'];
    if (rawItems is! List) {
      use.fail(
        const AzurePayloadError(
          'Azure não retornou Work Items vinculados válidos.',
        ),
      );
    }
    final ids = <int>[];
    for (final rawItem in rawItems) {
      final item = objectMap(rawItem);
      final id = _integerId(item?['id']);
      if (item == null || id == null) {
        use.fail(const AzurePayloadError('ID de Work Item inválido.'));
      }
      ids.add(id);
    }
    return ids;
  });
}

AppEffect<T> _decode<T extends Object>(T Function() decode) => Effect.tryAsync(
  decode,
  onError: (error, _) => AzurePayloadError('Resposta Azure inválida: $error'),
);
