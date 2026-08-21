import 'dart:convert';

import 'package:better_effect/better_effect.dart';
import 'package:dio/dio.dart';

import '../../app/app_effect.dart';
import '../../app/app_failure.dart';

final class AzureClientOptions {
  const AzureClientOptions({
    required this.pat,
    required this.organization,
    this.baseUrl,
  });

  final String pat;
  final String organization;
  final String? baseUrl;
}

final class AzureRequest {
  const AzureRequest(
    this.path, {
    this.method = 'GET',
    this.body,
    this.contentType = 'application/json',
  });

  final String path;
  final String method;
  final Object? body;
  final String contentType;
}

final class AzureHttpResponse {
  const AzureHttpResponse({
    required this.status,
    required this.body,
    required this.data,
  });

  final int status;
  final String body;
  final Object? data;
}

base class AzureFailure extends AppFailure {
  const AzureFailure(String message) : super(message, 1);
}

final class AzureConfigurationError extends AzureFailure {
  const AzureConfigurationError(super.message);
}

final class AzureTransportError extends AzureFailure {
  const AzureTransportError(super.message);
}

final class AzurePayloadError extends AzureFailure {
  const AzurePayloadError(super.message);
}

final class AzureApiError extends AzureFailure {
  AzureApiError(this.status, this.responseBody)
    : super(
        'Azure DevOps API respondeu $status${responseBody.isEmpty ? '' : ': $responseBody'}',
      );

  final int status;
  final String responseBody;
}

abstract interface class AzureHttp {
  AppEffect<AzureHttpResponse> send(AzureRequest request);
}

abstract interface class AzureDevOpsClient {
  AppEffect<AzureHttpResponse> request(AzureRequest request);
}

final class AzureHttpLive implements AzureHttp {
  const AzureHttpLive();

  @override
  AppEffect<AzureHttpResponse> send(AzureRequest request) => Effect.result((
    use,
  ) async {
    final config = use<AzureClientOptions>();
    if (config.organization.trim().isEmpty) {
      use.fail(
        const AzureConfigurationError(
          'Organização Azure DevOps não informada.',
        ),
      );
    }
    if (config.pat.trim().isEmpty) {
      use.fail(
        const AzureConfigurationError('PAT do Azure DevOps não configurado.'),
      );
    }

    final dio = use<Dio>();
    return use.unwrap(
      Effect.tryAsync(
        () async {
          final response = await dio.request<String>(
            azureUrl(config, request.path),
            data: request.body == null ? null : jsonEncode(request.body),
            options: Options(
              method: request.method,
              responseType: ResponseType.plain,
              validateStatus: (_) => true,
              headers: {
                'Accept': 'application/json',
                'Authorization':
                    'Basic ${base64Encode(utf8.encode(':${config.pat}'))}',
                'Content-Type': request.contentType,
              },
            ),
          );
          final status = response.statusCode ?? 0;
          final body = response.data ?? '';
          if (status < 200 || status >= 300) {
            throw AzureApiError(status, body.trim());
          }
          final payload = body.trim().isEmpty
              ? null
              : await use.unwrap(_decode(body).either());
          final data = payload?.fold<Object?>(
            (value) => value is _AzureJsonNull ? null : value,
            (_) => body,
          );
          return AzureHttpResponse(status: status, body: body, data: data);
        },
        onError: (error, _) => switch (error) {
          AzureFailure failure => failure,
          DioException exception => AzureTransportError(
            exception.message ?? exception.type.name,
          ),
          Object value => AzureTransportError(value.toString()),
        },
      ),
    );
  });
}

final class AzureDevOpsClientLive implements AzureDevOpsClient {
  const AzureDevOpsClientLive();

  @override
  AppEffect<AzureHttpResponse> request(AzureRequest request) =>
      Effect.result((use) async {
        final http = use<AzureHttp>();
        return use.unwrap(http.send(request));
      });
}

String pathSegment(Object value) => Uri.encodeComponent('$value');

String withApiVersion(String path, [String version = '7.1']) {
  final separator = path.contains('?') ? '&' : '?';
  return '$path${separator}api-version=${Uri.encodeComponent(version)}';
}

String azureUrl(AzureClientOptions options, String path) {
  final defaultBase =
      'https://dev.azure.com/${Uri.encodeComponent(options.organization)}';
  final base = (options.baseUrl ?? defaultBase).replaceFirst(
    RegExp(r'\/$'),
    '',
  );
  return '$base/${path.replaceFirst(RegExp(r'^/+'), '')}';
}

final class _AzureJsonNull {
  const _AzureJsonNull();
}

AppEffect<Object> _decode(String body) => Effect.tryAsync(
  () => jsonDecode(body) ?? const _AzureJsonNull(),
  onError: (error, _) => AzurePayloadError('Resposta Azure inválida: $error'),
);
