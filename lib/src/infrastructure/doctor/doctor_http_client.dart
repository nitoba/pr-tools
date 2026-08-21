import 'package:better_effect/better_effect.dart';
import 'package:dio/dio.dart';

import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import '../../features/doctor/doctor_service.dart';

final class DoctorHttpClientLive implements DoctorHttpClient {
  const DoctorHttpClientLive();

  @override
  AppEffect<DoctorHttpResponse> get(
    String url, {
    required Map<String, String> headers,
    Duration timeout = doctorCommandTimeout,
  }) => Effect.result((use) async {
    final dio = use<Dio>();
    return use.tryAsync(() async {
      final response = await dio.get<String>(
        url,
        options: Options(
          responseType: ResponseType.plain,
          validateStatus: (_) => true,
          headers: headers,
          sendTimeout: timeout,
          receiveTimeout: timeout,
        ),
      );
      return DoctorHttpResponse(
        status: response.statusCode ?? 0,
        body: response.data ?? '',
      );
    }, onError: (error, _) => ProcessFailure(error.toString()));
  });
}
