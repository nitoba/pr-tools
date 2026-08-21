import 'dart:convert';

import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import '../../application/config/config_models.dart';
import '../../features/doctor/doctor_models.dart';
import '../../features/doctor/doctor_service.dart';
import '../../domain/change_context.dart';

final class DoctorAzureProbeLive implements DoctorAzureProbe {
  const DoctorAzureProbeLive();

  @override
  AppEffect<DoctorAzureReport> probe(
    Config config,
    RepositoryRemote remote,
  ) => Effect.result((use) async {
    final headers = {
      'Authorization':
          'Basic ${base64Encode(utf8.encode(':${config.azurePat}'))}',
    };
    final http = use<DoctorHttpClient>();
    final repository = await use.unwrap(
      http
          .get(
            _azureUrl(
              remote,
              '/${_pathSegment(remote.project)}/_apis/git/repositories/${_pathSegment(remote.repository)}',
            ),
            headers: headers,
          )
          .either(),
    );
    final workItems = await use.unwrap(
      http
          .get(
            _azureUrl(
              remote,
              '/${_pathSegment(remote.project)}/_apis/wit/workitemtypes',
            ),
            headers: headers,
          )
          .either(),
    );
    return DoctorAzureReport(
      repository: _probe(repository),
      workItems: _probe(workItems),
    );
  });
}

DoctorProbeResult _probe(ResultDart<DoctorHttpResponse, AppFailure> result) =>
    result.fold(
      (response) => DoctorProbeResult(
        ok: response.status >= 200 && response.status < 300,
        status: response.status,
      ),
      (failure) => DoctorProbeResult(ok: false, error: failure.message),
    );

String _azureUrl(RepositoryRemote remote, String path) =>
    'https://dev.azure.com/${_pathSegment(remote.organization)}$path?api-version=7.1';

String _pathSegment(Object value) => Uri.encodeComponent('$value');
