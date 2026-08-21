import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import 'client.dart';
import 'types.dart';

abstract interface class AzureWorkItemClient {
  AppEffect<AzureWorkItem> get(String project, int id);

  AppEffect<List<int>> query(String project, String wiql);

  AppEffect<AzureUnit> updateState(String project, int id, String state);

  AppEffect<AzureUnit> updateToTestQa(
    String project,
    int id, {
    num? effort,
    num? realEffort,
  });

  AppEffect<AzureWorkItem> createTestCase(
    String project,
    CreateTestCaseInput input,
  );
}

final class AzureWorkItemClientLive implements AzureWorkItemClient {
  const AzureWorkItemClientLive();

  @override
  AppEffect<AzureWorkItem> get(String project, int id) => Effect.result((
    use,
  ) async {
    final client = use<AzureDevOpsClient>();
    final response = await use.unwrap(
      client.request(
        AzureRequest(
          withApiVersion('/${pathSegment(project)}/_apis/wit/workitems/$id'),
        ),
      ),
    );
    final data = objectMap(response.data);
    if (data == null) {
      use.fail(
        const AzurePayloadError('Azure não retornou um Work Item válido.'),
      );
    }
    return use.unwrap(_decode(() => AzureWorkItem.fromJson(data)));
  });

  @override
  AppEffect<List<int>> query(String project, String wiql) =>
      Effect.result((use) async {
        final client = use<AzureDevOpsClient>();
        final response = await use.unwrap(
          client.request(
            AzureRequest(
              withApiVersion('/${pathSegment(project)}/_apis/wit/wiql'),
              method: 'POST',
              body: {'query': wiql},
            ),
          ),
        );
        final data = objectMap(response.data);
        final rawItems = data?['workItems'];
        if (rawItems is! List) {
          use.fail(
            const AzurePayloadError('Azure não retornou Work Items válidos.'),
          );
        }
        final ids = <int>[];
        for (final rawItem in rawItems) {
          final item = objectMap(rawItem);
          final id = _integerId(item?['id']);
          if (item == null || id == null) {
            use.fail(
              const AzurePayloadError(
                'Azure retornou um ID de Work Item inválido.',
              ),
            );
          }
          ids.add(id);
        }
        return ids;
      });

  @override
  AppEffect<AzureUnit> updateState(String project, int id, String state) =>
      Effect.result((use) async {
        final client = use<AzureDevOpsClient>();
        await use.unwrap(
          client.request(
            AzureRequest(
              withApiVersion(
                '/${pathSegment(project)}/_apis/wit/workitems/$id',
              ),
              method: 'PATCH',
              contentType: 'application/json-patch+json',
              body: [
                {'op': 'add', 'path': '/fields/System.State', 'value': state},
              ],
            ),
          ),
        );
        return const AzureUnit();
      });

  @override
  AppEffect<AzureUnit> updateToTestQa(
    String project,
    int id, {
    num? effort,
    num? realEffort,
  }) => Effect.result((use) async {
    final client = use<AzureDevOpsClient>();
    final body = <Map<String, Object?>>[
      {'op': 'add', 'path': '/fields/System.State', 'value': 'Test QA'},
    ];
    if (effort != null) {
      body.add({
        'op': 'add',
        'path': '/fields/Microsoft.VSTS.Scheduling.Effort',
        'value': effort,
      });
    }
    if (realEffort != null) {
      body.add({
        'op': 'add',
        'path': '/fields/Custom.RealEffort',
        'value': realEffort,
      });
    }
    await use.unwrap(
      client.request(
        AzureRequest(
          withApiVersion('/${pathSegment(project)}/_apis/wit/workitems/$id'),
          method: 'PATCH',
          contentType: 'application/json-patch+json',
          body: body,
        ),
      ),
    );
    return const AzureUnit();
  });

  @override
  AppEffect<AzureWorkItem> createTestCase(
    String project,
    CreateTestCaseInput input,
  ) => Effect.result((use) async {
    final client = use<AzureDevOpsClient>();
    final body = <Map<String, Object?>>[
      {'op': 'add', 'path': '/fields/System.Title', 'value': input.title},
    ];
    final fields = <(String, Object?)>[
      ('/fields/System.Description', input.descriptionHtml),
      ('/fields/Microsoft.VSTS.TCM.Steps', input.stepsXml),
      ('/fields/System.AreaPath', input.areaPath),
      ('/fields/System.IterationPath', input.iterationPath),
      ('/fields/Microsoft.VSTS.Common.Priority', input.priority),
      ('/fields/Custom.Team', input.team),
      ('/fields/Custom.ProgramasAgrotrace', input.program),
      ('/fields/System.AssignedTo', input.assignedTo),
    ];
    for (final (path, value) in fields) {
      if (value != null && value != '') {
        body.add({'op': 'add', 'path': path, 'value': value});
      }
    }
    if (input.parentId case final parentId? when parentId > 0) {
      final config = use<AzureClientOptions>();
      body.add({
        'op': 'add',
        'path': '/relations/-',
        'value': {
          'rel': 'System.LinkTypes.Hierarchy-Reverse',
          'url': azureUrl(config, '/_apis/wit/workitems/$parentId'),
        },
      });
    }
    final response = await use.unwrap(
      client.request(
        AzureRequest(
          withApiVersion(
            '/${pathSegment(project)}/_apis/wit/workitems/${pathSegment(r'$Test Case')}',
          ),
          method: 'POST',
          contentType: 'application/json-patch+json',
          body: body,
        ),
      ),
    );
    final data = objectMap(response.data);
    if (data == null) {
      use.fail(
        const AzurePayloadError('Azure não retornou o Test Case criado.'),
      );
    }
    return use.unwrap(_decode(() => AzureWorkItem.fromJson(data)));
  });
}

String workItemField(AzureWorkItem item, String key) {
  final value = item.fields[key];
  return value is String ? value : '';
}

AppEffect<T> _decode<T extends Object>(T Function() decode) => Effect.tryAsync(
  decode,
  onError: (error, _) => AzurePayloadError('Resposta Azure inválida: $error'),
);

String workItemTitle(AzureWorkItem item) => workItemField(item, 'System.Title');

String workItemType(AzureWorkItem item) =>
    workItemField(item, 'System.WorkItemType');

String workItemDescription(AzureWorkItem item) =>
    workItemField(item, 'System.Description');

String workItemSprint(AzureWorkItem item) {
  final path = workItemField(item, 'System.IterationPath');
  final tokens = path
      .split(RegExp(r'[\\/ ]'))
      .where((value) => value.isNotEmpty);
  return tokens.toList().reversed.firstWhere(
    (value) => RegExp(r'^\d+$').hasMatch(value),
    orElse: () => '',
  );
}

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
