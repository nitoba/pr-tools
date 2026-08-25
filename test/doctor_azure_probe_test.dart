import 'dart:async';

import 'package:better_effect/better_effect.dart';
import 'package:pr_tools/src/app/app_effect.dart';
import 'package:pr_tools/src/application/config/config_models.dart';
import 'package:pr_tools/src/domain/change_context.dart';
import 'package:pr_tools/src/features/doctor/doctor_service.dart';
import 'package:pr_tools/src/infrastructure/doctor/doctor_azure_probe.dart';
import 'package:test/test.dart';

void main() {
  test(
    'starts independent Azure checks together and keeps both results',
    () async {
      final client = _GatedHttpClient();
      final result =
          await Module([
            .instance<DoctorHttpClient>(client),
            .provide<DoctorAzureProbe>(DoctorAzureProbeLive.new),
          ]).run(
            Effect.result(
              (use) =>
                  use.unwrap(use<DoctorAzureProbe>().probe(_config, _remote)),
            ),
          );

      final report = result.fold((value) => value, (failure) => throw failure);
      expect(client.urls, hasLength(2));
      expect(report.repository.ok, isTrue);
      expect(report.repository.status, 200);
      expect(report.workItems.ok, isTrue);
      expect(report.workItems.status, 200);
    },
  );
}

final class _GatedHttpClient implements DoctorHttpClient {
  final _bothStarted = Completer<void>();
  final urls = <String>[];

  @override
  AppEffect<DoctorHttpResponse> get(
    String url, {
    required Map<String, String> headers,
    Duration timeout = doctorCommandTimeout,
  }) => Effect.result((_) async {
    urls.add(url);
    if (urls.length == 2) _bothStarted.complete();
    await _bothStarted.future.timeout(const Duration(seconds: 1));
    return const DoctorHttpResponse(status: 200, body: '');
  });
}

const _config = Config(
  providers: ['codex'],
  baseUrl: 'https://api.openai.com/v1',
  compatibleModel: 'model',
  compatibleReasoning: 'provider-default',
  codexModel: 'model',
  codexReasoning: 'provider-default',
  opencodeModel: 'model',
  opencodeReasoning: 'provider-default',
  azurePat: 'pat',
  reviewerDev: '',
  reviewerSprint: '',
  testAreaPath: '',
  testAssignedTo: '',
  testTeam: '',
  testProgram: '',
  apiKey: '',
  template: '',
);

const _remote = RepositoryRemote(
  organization: 'acme',
  project: 'project',
  repository: 'repo',
);
