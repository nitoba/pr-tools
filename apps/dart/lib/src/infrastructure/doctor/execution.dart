import 'package:better_effect/better_effect.dart';
import 'package:dio/dio.dart';

import '../../features/doctor/doctor_service.dart';
import 'doctor_azure_probe.dart';
import 'doctor_http_client.dart';

Module doctorRequestModule() => Module([
  .resource<Dio>(
    acquire: (_) => Dio(),
    release: (dio, _) => dio.close(force: true),
  ),
  .provide<DoctorHttpClient>(DoctorHttpClientLive.new),
  .provide<DoctorAzureProbe>(DoctorAzureProbeLive.new),
]);
